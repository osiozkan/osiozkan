//! Uygulama içi ses oynatma: ffmpeg ile çözülen PCM, cpal çıkışına (özellik `audio`) beslenir.
//! Oynatılan örnek sayısı hassas bir "ses saati" verir; video/imleç bu saate bağlanır.
use std::collections::VecDeque;
use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Ses aygıtı geri çağrısı ile okuma iş parçacığı arasında paylaşılan durum.
pub struct Shared {
    buf: Mutex<VecDeque<f32>>,
    played_frames: AtomicU64,
    reader_done: AtomicBool,
    stop: AtomicBool,
    pub channels: u16,
    pub sample_rate: u32,
    latency_us: AtomicU64,
}

impl Shared {
    pub fn new(channels: u16, sample_rate: u32) -> Arc<Shared> {
        Arc::new(Shared { buf: Mutex::new(VecDeque::new()), played_frames: AtomicU64::new(0), reader_done: AtomicBool::new(false), stop: AtomicBool::new(false), channels, sample_rate, latency_us: AtomicU64::new(0) })
    }

    /// Aygıt geri çağrısı: `out` interleaved f32; yetmeyen kısım sessizlikle doldurulur.
    pub fn fill(&self, out: &mut [f32]) {
        let mut b = self.buf.lock().unwrap();
        let n = out.len().min(b.len());
        for (o, s) in out.iter_mut().zip(b.drain(..n)) {
            *o = s;
        }
        for o in out[n..].iter_mut() {
            *o = 0.0;
        }
        self.played_frames.fetch_add((n / self.channels.max(1) as usize) as u64, Ordering::SeqCst);
    }

    pub fn push(&self, samples: &[f32]) {
        self.buf.lock().unwrap().extend(samples.iter().copied());
    }

    pub fn buffered(&self) -> usize {
        self.buf.lock().unwrap().len()
    }

    /// Oynatılan süre (ms), aygıt gecikmesi çıkarılmış.
    pub fn elapsed_ms(&self) -> i64 {
        let ms = self.played_frames.load(Ordering::SeqCst) as f64 * 1000.0 / self.sample_rate.max(1) as f64;
        (ms - self.latency_us.load(Ordering::SeqCst) as f64 / 1000.0).max(0.0) as i64
    }

    pub fn finished(&self) -> bool {
        self.reader_done.load(Ordering::SeqCst) && self.buffered() == 0
    }
}

pub struct AudioPlayer {
    shared: Arc<Shared>,
    child: Option<Child>,
    start_ms: i64,
    #[cfg(feature = "audio")]
    _stream: cpal::Stream,
}

impl AudioPlayer {
    /// Başlangıç noktası (ms) + ses saatine göre geçerli konum.
    pub fn position_ms(&self) -> i64 {
        self.start_ms + self.shared.elapsed_ms()
    }

    pub fn finished(&self) -> bool {
        self.shared.finished()
    }

    /// ffmpeg çıktısını `shared`'a akıtır; tampon 2 sn'yi aşınca bekler.
    fn spawn_reader(path: &Path, start_ms: i64, dur_ms: i64, shared: Arc<Shared>) -> Result<Child, String> {
        let mut child = Command::new(crate::ffmpeg_bin())
            .args(["-v", "error", "-ss", &format!("{:.3}", start_ms.max(0) as f64 / 1000.0), "-i"])
            .arg(path)
            .args(["-t", &format!("{:.3}", dur_ms.max(1) as f64 / 1000.0), "-vn", "-sn", "-f", "f32le", "-ac", &shared.channels.to_string(), "-ar", &shared.sample_rate.to_string(), "-"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("ffmpeg çalıştırılamadı: {}", e))?;
        let mut out = child.stdout.take().ok_or("stdout yok")?;
        std::thread::spawn(move || {
            let mut chunk = vec![0u8; 16384];
            let mut carry: Vec<u8> = Vec::new();
            let limit = (shared.sample_rate as usize) * shared.channels as usize * 2;
            loop {
                if shared.stop.load(Ordering::SeqCst) {
                    break;
                }
                if shared.buffered() > limit {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                    continue;
                }
                match out.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        carry.extend_from_slice(&chunk[..n]);
                        let whole = carry.len() / 4 * 4;
                        let samples: Vec<f32> = carry[..whole].chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
                        carry.drain(..whole);
                        shared.push(&samples);
                    }
                }
            }
            shared.reader_done.store(true, Ordering::SeqCst);
        });
        Ok(child)
    }

    #[cfg(feature = "audio")]
    pub fn start(path: &Path, start_ms: i64, dur_ms: i64) -> Result<AudioPlayer, String> {
        use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
        let host = cpal::default_host();
        let device = host.default_output_device().ok_or("ses çıkış aygıtı yok")?;
        let cfg = device.default_output_config().map_err(|e| format!("ses yapılandırması: {}", e))?;
        let (channels, rate, fmt) = (cfg.channels(), cfg.sample_rate(), cfg.sample_format());
        let shared = Shared::new(channels, rate);
        let sc = cpal::StreamConfig { channels, sample_rate: rate, buffer_size: cpal::BufferSize::Default };

        fn build<T: cpal::SizedSample + cpal::FromSample<f32> + Send + 'static>(device: &cpal::Device, sc: cpal::StreamConfig, shared: Arc<Shared>) -> Result<cpal::Stream, String> {
            use cpal::traits::DeviceTrait;
            let mut scratch: Vec<f32> = Vec::new();
            device
                .build_output_stream::<T, _, _>(
                    sc,
                    move |data: &mut [T], info: &cpal::OutputCallbackInfo| {
                        scratch.resize(data.len(), 0.0);
                        shared.fill(&mut scratch);
                        for (o, s) in data.iter_mut().zip(scratch.iter()) {
                            *o = T::from_sample(*s);
                        }
                        let ts = info.timestamp();
                        let lat = ts.playback.duration_since(ts.callback);
                        shared.latency_us.store(lat.as_micros() as u64, Ordering::SeqCst);
                    },
                    |e| eprintln!("ses akışı hatası: {}", e),
                    None,
                )
                .map_err(|e| format!("ses akışı açılamadı: {}", e))
        }
        let stream = match fmt {
            cpal::SampleFormat::F32 => build::<f32>(&device, sc, shared.clone())?,
            cpal::SampleFormat::I16 => build::<i16>(&device, sc, shared.clone())?,
            cpal::SampleFormat::U16 => build::<u16>(&device, sc, shared.clone())?,
            other => return Err(format!("desteklenmeyen örnek biçimi: {:?}", other)),
        };
        let child = Self::spawn_reader(path, start_ms, dur_ms, shared.clone())?;
        stream.play().map_err(|e| format!("ses başlatılamadı: {}", e))?;
        Ok(AudioPlayer { shared, child: Some(child), start_ms, _stream: stream })
    }

    #[cfg(not(feature = "audio"))]
    pub fn start(_path: &Path, _start_ms: i64, _dur_ms: i64) -> Result<AudioPlayer, String> {
        Err("ses çıkışı bu derlemede yok (özellik `audio`)".into())
    }
}

impl Drop for AudioPlayer {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_follows_consumed_frames() {
        let s = Shared::new(2, 1000);
        s.push(&vec![0.5; 2 * 500]); // 500 kare = 0.5 sn
        let mut out = vec![1.0f32; 2 * 300];
        s.fill(&mut out);
        assert!(out.iter().all(|&v| v == 0.5));
        assert_eq!(s.elapsed_ms(), 300);
        let mut out = vec![1.0f32; 2 * 300];
        s.fill(&mut out); // 200 kare var, kalan sessizlik; saat yalnızca gerçek kareleri sayar
        assert_eq!(s.elapsed_ms(), 500);
        assert_eq!(out[2 * 200], 0.0);
        s.reader_done.store(true, Ordering::SeqCst);
        assert!(s.finished());
    }

    #[test]
    fn reader_feeds_buffer_from_ffmpeg() {
        if !crate::ffmpeg_available() {
            return;
        }
        let dir = std::env::temp_dir().join(format!("osisub_audio_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("a.wav");
        let ok = Command::new(crate::ffmpeg_bin()).args(["-v", "error", "-y", "-f", "lavfi", "-i", "sine=frequency=440:duration=2", "-ar", "44100"]).arg(&f).status().unwrap().success();
        assert!(ok);
        let shared = Shared::new(2, 48000);
        let mut child = AudioPlayer::spawn_reader(&f, 500, 1000, shared.clone()).unwrap();
        // Okuyucu bitene kadar bekle
        let t = std::time::Instant::now();
        while !shared.reader_done.load(Ordering::SeqCst) && t.elapsed().as_secs() < 10 {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let _ = child.wait();
        // 1 sn × 48000 kare × 2 kanal
        let n = shared.buffered();
        assert!((n as i64 - 96000).abs() < 4800, "örnek sayısı {}", n);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
