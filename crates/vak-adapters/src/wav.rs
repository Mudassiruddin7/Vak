//! Just enough WAV handling: how long a recording is, and joining several clips into one.

/// (sample_rate, channels, bits, data range) of a PCM WAV.
fn parse(wav: &[u8]) -> Option<(u32, u16, u16, std::ops::Range<usize>)> {
    if wav.len() < 44 || &wav[0..4] != b"RIFF" || &wav[8..12] != b"WAVE" {
        return None;
    }
    let (mut pos, mut fmt) = (12usize, None);
    while pos + 8 <= wav.len() {
        let id = &wav[pos..pos + 4];
        let size = u32::from_le_bytes(wav[pos + 4..pos + 8].try_into().ok()?) as usize;
        let body = pos + 8;
        if id == b"fmt " && body + 16 <= wav.len() {
            let ch = u16::from_le_bytes(wav[body + 2..body + 4].try_into().ok()?);
            let rate = u32::from_le_bytes(wav[body + 4..body + 8].try_into().ok()?);
            let bits = u16::from_le_bytes(wav[body + 14..body + 16].try_into().ok()?);
            fmt = Some((rate, ch, bits));
        } else if id == b"data" {
            let end = (body + size).min(wav.len());
            let (rate, ch, bits) = fmt?;
            return Some((rate, ch, bits, body..end));
        }
        pos = body + size + (size & 1);
    }
    None
}

pub fn seconds(wav: &[u8]) -> Option<f64> {
    let (rate, ch, bits, data) = parse(wav)?;
    let bytes_per_sec = rate as f64 * ch as f64 * (bits as f64 / 8.0);
    (bytes_per_sec > 0.0).then(|| data.len() as f64 / bytes_per_sec)
}

pub fn pcm_wav(rate: u32, channels: u16, bits: u16, pcm: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(44 + pcm.len());
    let block = channels * bits / 8;
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + pcm.len() as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * block as u32).to_le_bytes());
    out.extend_from_slice(&block.to_le_bytes());
    out.extend_from_slice(&bits.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
    out.extend_from_slice(pcm);
    out
}

/// Join clips that share a format. Returns the first clip alone if the formats differ or any is not PCM.
pub fn merge(parts: Vec<Vec<u8>>) -> Vec<u8> {
    if parts.len() <= 1 {
        return parts.into_iter().next().unwrap_or_default();
    }
    let parsed: Vec<_> = parts.iter().map(|p| parse(p)).collect();
    let Some(Some((rate, ch, bits, _))) = parsed.first().cloned() else { return parts.into_iter().next().unwrap_or_default() };
    if parsed.iter().any(|p| p.as_ref().is_none_or(|(r, c, b, _)| (*r, *c, *b) != (rate, ch, bits))) {
        return parts.into_iter().next().unwrap_or_default();
    }
    let pcm: Vec<u8> = parts.iter().zip(&parsed).flat_map(|(p, d)| p[d.as_ref().expect("checked").3.clone()].to_vec()).collect();
    pcm_wav(rate, ch, bits, &pcm)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn length_and_merge() {
        let one_sec = pcm_wav(16000, 1, 16, &vec![0u8; 32000]);
        assert!((seconds(&one_sec).unwrap() - 1.0).abs() < 1e-6);
        let two = merge(vec![one_sec.clone(), one_sec.clone()]);
        assert!((seconds(&two).unwrap() - 2.0).abs() < 1e-6);
        assert_eq!(seconds(b"not a wav at all, definitely not, nope nope nope nope"), None);
        let other = pcm_wav(24000, 1, 16, &vec![0u8; 100]);
        assert_eq!(merge(vec![one_sec.clone(), other]), one_sec, "mismatched formats are not spliced");
    }
}
