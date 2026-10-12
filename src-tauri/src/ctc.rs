//! Pré-traitement audio et décodage CTC pour wav2vec2 / MMS.
//! Aucune dépendance au runtime ONNX : cette partie est testée à part (`cargo test`).

use serde_json::Value;

/// Métadonnées exportées par `export_onnx.py` (meta.json + vocab.json).
pub struct Meta {
    pub vocab_size: usize,
    pub pad_id: usize,
    pub delimiter: String,
    pub sample_rate: u32,
    pub do_normalize: bool,
    /// id -> token (chaîne vide si l'id n'a pas de token)
    pub tokens: Vec<String>,
}

pub fn load_meta(meta_json: &str, vocab_json: &str) -> Result<Meta, String> {
    let m: Value =
        serde_json::from_str(meta_json).map_err(|e| format!("meta.json invalide : {e}"))?;
    let vocab: Value =
        serde_json::from_str(vocab_json).map_err(|e| format!("vocab.json invalide : {e}"))?;

    let vocab_size = m["vocab_size"]
        .as_u64()
        .ok_or("meta.json : vocab_size manquant")? as usize;
    let pad_id = m["pad_token_id"]
        .as_u64()
        .ok_or("meta.json : pad_token_id manquant")? as usize;
    if pad_id >= vocab_size {
        return Err("meta.json : pad_token_id hors du vocabulaire".into());
    }

    let obj = vocab
        .as_object()
        .ok_or("vocab.json : un objet {token: id} est attendu")?;
    let mut tokens = vec![String::new(); vocab_size];
    for (token, id) in obj {
        let id = id
            .as_u64()
            .ok_or("vocab.json : les valeurs doivent être des entiers (vocabulaire à plat)")?
            as usize;
        if id < vocab_size {
            tokens[id] = token.clone();
        }
    }

    Ok(Meta {
        vocab_size,
        pad_id,
        delimiter: m["word_delimiter_token"]
            .as_str()
            .unwrap_or("|")
            .to_string(),
        sample_rate: m["sample_rate"].as_u64().unwrap_or(16_000) as u32,
        do_normalize: m["do_normalize"].as_bool().unwrap_or(true),
        tokens,
    })
}

/// Lit un WAV PCM 16 bits ou float 32 bits ; renvoie (échantillons mono, fréquence).
pub fn parse_wav(bytes: &[u8]) -> Result<(Vec<f32>, u32), String> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("Fichier WAV invalide".into());
    }

    let mut fmt: Option<(u16, u16, u32, u16)> = None; // (format, canaux, fréquence, bits)
    let mut data: Option<&[u8]> = None;
    let mut pos = 12usize;

    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = u32::from_le_bytes([
            bytes[pos + 4],
            bytes[pos + 5],
            bytes[pos + 6],
            bytes[pos + 7],
        ]) as usize;
        let start = pos + 8;
        let end = start.saturating_add(size).min(bytes.len());
        if id == b"fmt " && end - start >= 16 {
            let b = &bytes[start..end];
            fmt = Some((
                u16::from_le_bytes([b[0], b[1]]),
                u16::from_le_bytes([b[2], b[3]]),
                u32::from_le_bytes([b[4], b[5], b[6], b[7]]),
                u16::from_le_bytes([b[14], b[15]]),
            ));
        } else if id == b"data" {
            data = Some(&bytes[start..end]);
        }
        pos = start.saturating_add(size).saturating_add(size & 1);
    }

    let (mut format, channels, rate, bits) = fmt.ok_or("WAV sans bloc fmt")?;
    let data = data.ok_or("WAV sans données audio")?;
    if format == 0xFFFE {
        format = if bits == 32 { 3 } else { 1 }; // WAVE_FORMAT_EXTENSIBLE
    }
    if channels == 0 {
        return Err("WAV : nombre de canaux invalide".into());
    }

    let samples: Vec<f32> = match (format, bits) {
        (1, 16) => data
            .chunks_exact(2)
            .map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0)
            .collect(),
        (3, 32) => data
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect(),
        _ => return Err(format!("WAV non supporté (format {format}, {bits} bits)")),
    };

    let ch = channels as usize;
    let mono = if ch == 1 {
        samples
    } else {
        samples
            .chunks_exact(ch)
            .map(|f| f.iter().sum::<f32>() / ch as f32)
            .collect()
    };
    Ok((mono, rate))
}

fn sinc(x: f64) -> f64 {
    if x == 0.0 {
        1.0
    } else {
        let px = std::f64::consts::PI * x;
        px.sin() / px
    }
}

/// Rééchantillonnage par sinc fenêtré (Lanczos) avec filtre passe-bas quand on descend en fréquence.
pub fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || input.is_empty() {
        return input.to_vec();
    }
    const A: f64 = 8.0; // passages par zéro de chaque côté
    let ratio = from as f64 / to as f64;
    let cutoff = (1.0 / ratio).min(1.0);
    let half = (A / cutoff).ceil() as isize;
    let out_len = (input.len() as f64 / ratio).floor() as usize;

    (0..out_len)
        .map(|i| {
            let t = i as f64 * ratio;
            let center = t.floor() as isize;
            let (mut acc, mut norm) = (0.0f64, 0.0f64);
            for k in (center - half + 1)..=(center + half) {
                if k < 0 || k as usize >= input.len() {
                    continue;
                }
                let x = (t - k as f64) * cutoff;
                if x.abs() >= A {
                    continue;
                }
                let w = sinc(x) * sinc(x / A) * cutoff;
                acc += input[k as usize] as f64 * w;
                norm += w;
            }
            if norm.abs() < 1e-9 {
                0.0
            } else {
                (acc / norm) as f32
            }
        })
        .collect()
}

/// Même normalisation que le feature extractor de Hugging Face (moyenne nulle, variance 1).
pub fn normalize(x: &mut [f32]) {
    if x.is_empty() {
        return;
    }
    let n = x.len() as f64;
    let mean = x.iter().map(|&v| v as f64).sum::<f64>() / n;
    let var = x.iter().map(|&v| (v as f64 - mean).powi(2)).sum::<f64>() / n;
    let denom = (var + 1e-7).sqrt();
    for v in x.iter_mut() {
        *v = ((*v as f64 - mean) / denom) as f32;
    }
}

/// Découpe en fenêtres de `max_s` secondes maximum, en coupant au moment le plus calme
/// des trois dernières secondes de chaque fenêtre.
pub fn split_chunks(audio: &[f32], sr: u32, max_s: f32) -> Vec<(usize, usize)> {
    let max = (max_s * sr as f32) as usize;
    let look = ((3.0 * sr as f32) as usize).min(max / 2);
    let step = ((0.02 * sr as f32) as usize).max(1);
    let mut out = Vec::new();
    let mut start = 0usize;

    while max > 0 && audio.len() - start > max {
        let (lo, hi) = (start + max - look, start + max);
        let (mut best, mut best_e) = (hi, f32::MAX);
        let mut i = lo;
        while i + step <= hi {
            let e: f32 = audio[i..i + step].iter().map(|x| x * x).sum();
            if e < best_e {
                best_e = e;
                best = i + step / 2;
            }
            i += step;
        }
        out.push((start, best));
        start = best;
    }
    out.push((start, audio.len()));
    out
}

/// Décodage CTC glouton, identique à `processor.batch_decode(argmax(logits))` :
/// on fusionne les répétitions consécutives, puis on retire le token vide (pad).
pub fn decode_greedy(logits: &[f32], meta: &Meta) -> String {
    let v = meta.vocab_size;
    let frames = logits.len() / v;
    let mut out = String::new();
    let mut prev: Option<usize> = None;

    for f in 0..frames {
        let row = &logits[f * v..(f + 1) * v];
        let mut best = 0usize;
        for (i, &x) in row.iter().enumerate() {
            if x > row[best] {
                best = i;
            }
        }
        if Some(best) != prev {
            prev = Some(best);
            if best != meta.pad_id {
                out.push_str(&meta.tokens[best]);
            }
        }
    }
    out.replace(&meta.delimiter, " ").trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta() -> Meta {
        load_meta(
            r#"{"vocab_size":5,"pad_token_id":0,"word_delimiter_token":"|","sample_rate":16000,"do_normalize":true}"#,
            r#"{"<pad>":0,"|":1,"a":2,"b":3,"é":4}"#,
        )
        .unwrap()
    }

    fn onehot(ids: &[usize], v: usize) -> Vec<f32> {
        ids.iter()
            .flat_map(|&i| (0..v).map(move |j| if j == i { 5.0 } else { 0.0 }))
            .collect()
    }

    fn wav(samples: &[i16], rate: u32, channels: u16) -> Vec<u8> {
        let data_len = (samples.len() * 2) as u32;
        let mut b = Vec::new();
        b.extend(b"RIFF");
        b.extend((36 + data_len).to_le_bytes());
        b.extend(b"WAVEfmt ");
        b.extend(16u32.to_le_bytes());
        b.extend(1u16.to_le_bytes());
        b.extend(channels.to_le_bytes());
        b.extend(rate.to_le_bytes());
        b.extend((rate * 2 * channels as u32).to_le_bytes());
        b.extend((2 * channels).to_le_bytes());
        b.extend(16u16.to_le_bytes());
        b.extend(b"data");
        b.extend(data_len.to_le_bytes());
        for s in samples {
            b.extend(s.to_le_bytes());
        }
        b
    }

    #[test]
    fn decode_fusionne_les_repetitions_puis_retire_le_pad() {
        // a a <pad> a | b b <pad> |   ->  "aa b"  (le pad sépare deux « a » distincts)
        let logits = onehot(&[2, 2, 0, 2, 1, 3, 3, 0, 1], 5);
        assert_eq!(decode_greedy(&logits, &meta()), "aa b");
    }

    #[test]
    fn decode_gere_les_accents_et_le_silence() {
        assert_eq!(decode_greedy(&onehot(&[4, 0, 4], 5), &meta()), "éé");
        assert_eq!(decode_greedy(&onehot(&[0, 0, 0], 5), &meta()), "");
        assert_eq!(decode_greedy(&[], &meta()), "");
    }

    #[test]
    fn meta_refuse_un_vocabulaire_imbrique() {
        let r = load_meta(r#"{"vocab_size":5,"pad_token_id":0}"#, r#"{"fmp":{"a":2}}"#);
        assert!(r.is_err());
    }

    #[test]
    fn wav_mono_16_bits() {
        let (s, sr) = parse_wav(&wav(&[0, 16384, -16384, 32767], 48000, 1)).unwrap();
        assert_eq!(sr, 48000);
        assert_eq!(s.len(), 4);
        assert!((s[1] - 0.5).abs() < 1e-4 && (s[2] + 0.5).abs() < 1e-4);
    }

    #[test]
    fn wav_stereo_est_moyenne_en_mono() {
        let (s, _) = parse_wav(&wav(&[16384, -16384, 8192, 8192], 16000, 2)).unwrap();
        assert_eq!(s.len(), 2);
        assert!(s[0].abs() < 1e-4);
        assert!((s[1] - 0.25).abs() < 1e-4);
    }

    #[test]
    fn wav_invalide_est_refuse() {
        assert!(parse_wav(b"pas un wav").is_err());
    }

    #[test]
    fn normalisation_moyenne_nulle_variance_un() {
        let mut x: Vec<f32> = (0..1000)
            .map(|i| (i as f32 * 0.01).sin() * 0.3 + 0.2)
            .collect();
        normalize(&mut x);
        let mean = x.iter().sum::<f32>() / x.len() as f32;
        let var = x.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / x.len() as f32;
        assert!(mean.abs() < 1e-4, "moyenne {mean}");
        assert!((var - 1.0).abs() < 1e-3, "variance {var}");
    }

    fn tone(freq: f64, sr: u32, secs: f64) -> Vec<f32> {
        (0..(sr as f64 * secs) as usize)
            .map(|i| (2.0 * std::f64::consts::PI * freq * i as f64 / sr as f64).sin() as f32 * 0.5)
            .collect()
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
    }

    #[test]
    fn resample_48k_vers_16k_garde_la_parole() {
        let y = resample(&tone(440.0, 48000, 1.0), 48000, 16000);
        assert_eq!(y.len(), 16000);
        let mid = &y[2000..14000]; // on ignore les bords
        assert!(
            (rms(mid) - 0.5 / 2f32.sqrt()).abs() < 0.01,
            "rms {}",
            rms(mid)
        );
    }

    #[test]
    fn resample_attenue_ce_qui_depasse_8_khz() {
        // 12 kHz serait replié en 4 kHz sans filtre passe-bas
        let y = resample(&tone(12000.0, 48000, 1.0), 48000, 16000);
        assert!(rms(&y[2000..14000]) < 0.02, "rms {}", rms(&y[2000..14000]));
    }

    #[test]
    fn resample_meme_frequence_ne_change_rien() {
        let x = tone(300.0, 16000, 0.1);
        assert_eq!(resample(&x, 16000, 16000), x);
    }

    #[test]
    fn decoupe_au_silence() {
        let sr = 16000u32;
        let mut a = tone(200.0, sr, 40.0);
        for v in &mut a[(23.2 * sr as f64) as usize..(23.8 * sr as f64) as usize] {
            *v = 0.0;
        }
        let chunks = split_chunks(&a, sr, 25.0);
        assert!(chunks.len() >= 2);
        let cut = chunks[0].1 as f64 / sr as f64;
        assert!(cut > 23.2 && cut < 23.8, "coupure à {cut}");
        assert_eq!(chunks[0].0, 0);
        assert_eq!(chunks.last().unwrap().1, a.len());
        for w in chunks.windows(2) {
            assert_eq!(w[0].1, w[1].0); // contigus
        }
        assert!(
            chunks
                .iter()
                .all(|(s, e)| e - s <= (25.0 * sr as f64) as usize)
        );
    }

    #[test]
    fn audio_court_donne_un_seul_morceau() {
        assert_eq!(
            split_chunks(&vec![0.1; 16000 * 5], 16000, 25.0),
            vec![(0, 80000)]
        );
    }
}
