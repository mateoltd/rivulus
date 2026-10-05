//! `zlib-stream` incremental inflate (resync buffer, dual caps, reset per connection).
//!
//! One `ZlibStream` per connection; call `reset()` on reconnect/resume.
//! Compressed bytes accumulate in a resync buffer until the
//! `Z_SYNC_FLUSH` suffix `00 00 FF FF` appears (it may split across WS
//! frames). Each suffix-delimited chunk is inflated through a persistent
//! `flate2` stream.

use flate2::Decompress;
use flate2::FlushDecompress;
use flate2::Status;

/// Max compressed bytes accepted per WS frame (error + reset on exceed).
pub const FRAME_CAP: usize = 256 * 1024;
/// Max inflated bytes per gateway message (legit READY/GUILD_CREATE are 1-5 MiB).
pub const MESSAGE_CAP: usize = 8 * 1024 * 1024;
/// Suffix marking flush boundary (may SPLIT across frames).
pub const ZSYNC_FLUSH: [u8; 4] = [0x00, 0x00, 0xFF, 0xFF];

/// Streaming decompressor (one per connection; `reset` on reconnect/resume).
#[derive(Debug)]
pub struct ZlibStream {
    decomp: Decompress,
    pending: Vec<u8>,
}

impl ZlibStream {
    /// New.
    #[must_use]
    pub fn new() -> Self {
        Self {
            decomp: Decompress::new(true),
            pending: Vec::new(),
        }
    }
    /// Reset after reconnect (fresh zlib stream, drop buffered bytes).
    pub fn reset(&mut self) {
        self.decomp = Decompress::new(true);
        self.pending.clear();
    }
    /// Feed a WS frame; returns complete messages (possibly zero).
    ///
    /// Buffers until `ZSYNC_FLUSH` is seen, so a suffix split across
    /// frames still resyncs. Caps enforced with error + reset.
    ///
    /// # Errors
    /// Returns [`common::Error::Network`] when caps are exceeded or
    /// inflate fails (state is reset before returning).
    pub fn feed(&mut self, frame: &[u8]) -> Result<Vec<Vec<u8>>, common::Error> {
        if frame.len() > FRAME_CAP {
            self.reset();
            return Err(common::Error::Network(Box::from("frame cap exceeded")));
        }
        if self.pending.len().saturating_add(frame.len()) > MESSAGE_CAP {
            self.reset();
            return Err(common::Error::Network(Box::from("buffer cap exceeded")));
        }
        self.pending.extend_from_slice(frame);
        let mut out = Vec::new();
        while let Some(pos) = find_suffix(&self.pending) {
            let chunk: Vec<u8> = self.pending.drain(..pos + 4).collect();
            match self.inflate_chunk(&chunk) {
                Ok(msg) => out.push(msg),
                Err(e) => {
                    self.reset();
                    return Err(e);
                }
            }
        }
        Ok(out)
    }

    fn inflate_chunk(&mut self, chunk: &[u8]) -> Result<Vec<u8>, common::Error> {
        let mut total = Vec::new();
        let mut input = chunk;
        let mut buf = [0u8; 32 * 1024];
        loop {
            if total.len() > MESSAGE_CAP {
                return Err(common::Error::Network(Box::from("message cap exceeded")));
            }
            let before_in = self.decomp.total_in();
            let before_out = self.decomp.total_out();
            match self
                .decomp
                .decompress(input, &mut buf, FlushDecompress::Sync)
            {
                Ok(status) => {
                    let consumed = self.decomp.total_in().saturating_sub(before_in) as usize;
                    let produced = self.decomp.total_out().saturating_sub(before_out) as usize;
                    let take_in = consumed.min(input.len());
                    let take_out = produced.min(buf.len());
                    if take_in < input.len() {
                        let (_, rest) = input.split_at(take_in);
                        input = rest;
                    } else {
                        input = &[];
                    }
                    total.extend_from_slice(&buf[..take_out]);
                    if total.len() > MESSAGE_CAP {
                        return Err(common::Error::Network(Box::from("message cap exceeded")));
                    }
                    match status {
                        Status::StreamEnd => break,
                        Status::Ok | Status::BufError => {
                            if input.is_empty() {
                                break;
                            }
                            if consumed == 0 && produced == 0 {
                                break;
                            }
                        }
                    }
                }
                Err(_) => {
                    return Err(common::Error::Network(Box::from("inflate failed")));
                }
            }
        }
        Ok(total)
    }
}

impl Default for ZlibStream {
    fn default() -> Self {
        Self::new()
    }
}

fn find_suffix(hay: &[u8]) -> Option<usize> {
    hay.windows(4).position(|w| w == ZSYNC_FLUSH)
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::Compress;
    use flate2::Compression;
    use flate2::FlushCompress;
    use flate2::Status;

    fn compress_many(payloads: &[&[u8]]) -> Vec<u8> {
        let mut c = Compress::new(Compression::default(), true);
        let mut out = Vec::new();
        let mut buf = [0u8; 1024];
        for payload in payloads {
            let mut input: &[u8] = payload;
            loop {
                let before_in = c.total_in();
                let before_out = c.total_out();
                let flush = if input.is_empty() {
                    FlushCompress::Sync
                } else {
                    FlushCompress::None
                };
                let status = c.compress(input, &mut buf, flush).expect("compress");
                let consumed = (c.total_in() - before_in) as usize;
                let produced = (c.total_out() - before_out) as usize;
                let take_in = consumed.min(input.len());
                let take_out = produced.min(buf.len());
                if take_in < input.len() {
                    let (_, rest) = input.split_at(take_in);
                    input = rest;
                } else {
                    input = &[];
                }
                out.extend_from_slice(&buf[..take_out]);
                match status {
                    Status::Ok => {
                        if input.is_empty() && flush == FlushCompress::Sync {
                            break;
                        }
                    }
                    Status::BufError | Status::StreamEnd => {
                        if input.is_empty() {
                            break;
                        }
                    }
                }
                if input.is_empty() && flush == FlushCompress::Sync {
                    break;
                }
            }
        }
        out
    }

    fn compress_one(payload: &[u8]) -> Vec<u8> {
        compress_many(&[payload])
    }

    #[test]
    fn suffix_split_detected() {
        let mut h = vec![1u8, 2u8, 0x00, 0x00];
        assert!(find_suffix(&h).is_none());
        h.extend_from_slice(&[0xFF, 0xFF, 9]);
        assert_eq!(find_suffix(&h), Some(2));
    }

    #[test]
    fn frame_cap_enforced() {
        let mut z = ZlibStream::new();
        let big = vec![0u8; FRAME_CAP + 1];
        assert!(z.feed(&big).is_err());
        assert!(z.pending.is_empty());
    }

    #[test]
    fn split_suffix_across_frames() {
        let payload = br#"{"op":10,"d":{"heartbeat_interval":41250}}"#;
        let compressed = compress_one(payload);
        assert!(compressed.windows(4).any(|w| w == ZSYNC_FLUSH));
        let split = compressed.len() - 2;
        let (a, b) = compressed.split_at(split);
        let mut z = ZlibStream::new();
        let first = z.feed(a).expect("first");
        assert!(first.is_empty(), "suffix split: no message yet");
        let second = z.feed(b).expect("second");
        assert_eq!(second.len(), 1);
        assert_eq!(second[0], payload);
    }

    #[test]
    fn two_messages_two_flushes() {
        let p1 = br#"{"op":11}"#;
        let p2 = br#"{"op":1,"d":42}"#;
        let combined = compress_many(&[p1.as_slice(), p2.as_slice()]);
        let mut z = ZlibStream::new();
        let msgs = z.feed(&combined).expect("feed");
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0], p1);
        assert_eq!(msgs[1], p2);
    }

    #[test]
    fn message_cap_enforced() {
        let mut z = ZlibStream::new();
        z.pending = vec![0u8; MESSAGE_CAP - 10];
        assert!(z.feed(&[9u8; 20]).is_err());
        assert!(z.pending.is_empty());
    }
}
