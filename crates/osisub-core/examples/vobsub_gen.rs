// VobSub test akışı üretir (ffmpeg ile çapraz doğrulama için):
// cargo run -p osisub-core --example vobsub_gen -- out.sub
fn main() {
    let out = std::env::args().nth(1).expect("çıktı");
    let (w, h) = (300usize, 61usize);
    let px: Vec<u8> = (0..w * h)
        .map(|i| {
            let (x, y) = (i % w, i / w);
            if x < 4 || y < 4 || x >= w - 4 || y >= h - 4 {
                2
            } else if (x / 10 + y / 10) % 2 == 0 {
                1
            } else {
                3
            }
        })
        .collect();
    let p = osisub_core::vobsub::encode_spu(211, 333, w as u32, h as u32, &px, [0, 1, 2, 3], [0, 15, 15, 15], 3000);
    std::fs::write(&out, osisub_core::vobsub::mux(&[(0, 1000, p)])).unwrap();
}
