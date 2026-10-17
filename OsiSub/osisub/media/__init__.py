"""Arayüzden bağımsız medya çekirdeği.

  probe        ffprobe ile akış bilgisi
  frameindex   paketleri çözmeden kare zaman damgası dizini (+ anahtar kareler)
  audio        ffmpeg ile mono PCM çözme, tepe zarfı (Peaks), spektrogram
  frames       tek kareyi tam doğrulukla çözme (aegisub.get_frame için)
  cache        <kullanıcı önbelleği>/media altında boyut+mtime anahtarlı önbellek
"""
