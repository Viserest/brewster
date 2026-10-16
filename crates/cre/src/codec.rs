//! Little-endian binary reading and writing shared by the page protocol's messages
//! (network requests and responses). Same conventions as `tree::wire`: `u32` lengths,
//! UTF-8 strings. Reading checks every length against what is left, so a bad message is an
//! error, never a huge allocation or a panic.

pub(crate) struct Writer(pub Vec<u8>);

impl Writer {
    pub fn new() -> Writer {
        Writer(Vec::new())
    }
    pub fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    pub fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    pub fn u32(&mut self, v: usize) {
        self.0.extend_from_slice(&(v as u32).to_le_bytes());
    }
    pub fn bytes(&mut self, b: &[u8]) {
        self.u32(b.len());
        self.0.extend_from_slice(b);
    }
    pub fn str(&mut self, s: &str) {
        self.bytes(s.as_bytes());
    }
}

pub(crate) struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(bytes: &'a [u8]) -> Reader<'a> {
        Reader { bytes, pos: 0 }
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.pos.checked_add(n).filter(|&e| e <= self.bytes.len());
        let end = end.ok_or_else(|| "message is cut short".to_string())?;
        let s = &self.bytes[self.pos..end];
        self.pos = end;
        Ok(s)
    }
    pub fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    pub fn u16(&mut self) -> Result<u16, String> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }
    pub fn u32(&mut self) -> Result<usize, String> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
    }
    /// A count of items that each take at least `min` bytes: refused when the rest of the
    /// message cannot hold that many.
    pub fn count(&mut self, min: usize) -> Result<usize, String> {
        let n = self.u32()?;
        if n.saturating_mul(min.max(1)) > self.bytes.len() - self.pos {
            return Err("message is cut short".to_string());
        }
        Ok(n)
    }
    pub fn bytes(&mut self) -> Result<Vec<u8>, String> {
        let n = self.u32()?;
        Ok(self.take(n)?.to_vec())
    }
    pub fn str(&mut self) -> Result<String, String> {
        String::from_utf8(self.bytes()?).map_err(|_| "text is not UTF-8".to_string())
    }
}
