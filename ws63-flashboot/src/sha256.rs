//! SHA256 software implementation for flashboot image verification.
//!
//! Minimal no_std SHA256 — no heap, no alloc.
//! Used to verify the app image hash before jumping.

/// SHA256 context
pub struct Sha256 {
    state: [u32; 8],
    buf: [u8; 64],
    buf_len: usize,
    total_len: u64,
}

// SHA256 initial hash values (first 32 bits of fractional parts of sqrt(primes 2..19))
const H: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
    0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

// SHA256 round constants (first 32 bits of fractional parts of cube roots of primes 2..311)
const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5,
    0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3,
    0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc,
    0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
    0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
    0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3,
    0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5,
    0x391c0cd3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
    0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

impl Sha256 {
    pub fn new() -> Self {
        Self {
            state: H,
            buf: [0u8; 64],
            buf_len: 0,
            total_len: 0,
        }
    }

    pub fn update(&mut self, data: &[u8]) {
        self.total_len += data.len() as u64;
        let mut offset = 0;
        while offset < data.len() {
            let space = 64 - self.buf_len;
            let copy = core::cmp::min(space, data.len() - offset);
            self.buf[self.buf_len..self.buf_len + copy]
                .copy_from_slice(&data[offset..offset + copy]);
            self.buf_len += copy;
            offset += copy;
            if self.buf_len == 64 {
                self.compress();
                self.buf_len = 0;
            }
        }
    }

    pub fn finish(mut self) -> [u8; 32] {
        // Padding
        let total_bits = self.total_len * 8;
        self.buf[self.buf_len] = 0x80;
        self.buf_len += 1;
        if self.buf_len > 56 {
            self.buf[self.buf_len..64].fill(0);
            self.compress();
            self.buf_len = 0;
        }
        self.buf[self.buf_len..56].fill(0);
        // Append length in big-endian
        for i in 0..8 {
            self.buf[56 + i] = ((total_bits >> (56 - i * 8)) & 0xFF) as u8;
        }
        self.compress();

        let mut hash = [0u8; 32];
        for i in 0..8 {
            hash[i * 4] = ((self.state[i] >> 24) & 0xFF) as u8;
            hash[i * 4 + 1] = ((self.state[i] >> 16) & 0xFF) as u8;
            hash[i * 4 + 2] = ((self.state[i] >> 8) & 0xFF) as u8;
            hash[i * 4 + 3] = (self.state[i] & 0xFF) as u8;
        }
        hash
    }

    fn compress(&mut self) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = ((self.buf[i * 4] as u32) << 24)
                | ((self.buf[i * 4 + 1] as u32) << 16)
                | ((self.buf[i * 4 + 2] as u32) << 8)
                | (self.buf[i * 4 + 3] as u32);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7)
                ^ w[i - 15].rotate_right(18)
                ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17)
                ^ w[i - 2].rotate_right(19)
                ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }

        let mut a = self.state[0];
        let mut b = self.state[1];
        let mut c = self.state[2];
        let mut d = self.state[3];
        let mut e = self.state[4];
        let mut f = self.state[5];
        let mut g = self.state[6];
        let mut h = self.state[7];

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        self.state[0] = self.state[0].wrapping_add(a);
        self.state[1] = self.state[1].wrapping_add(b);
        self.state[2] = self.state[2].wrapping_add(c);
        self.state[3] = self.state[3].wrapping_add(d);
        self.state[4] = self.state[4].wrapping_add(e);
        self.state[5] = self.state[5].wrapping_add(f);
        self.state[6] = self.state[6].wrapping_add(g);
        self.state[7] = self.state[7].wrapping_add(h);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha256_empty() {
        let mut sha = Sha256::new();
        sha.update(b"");
        let hash = sha.finish();
        // SHA256("") = e3b0c44298fc1c14...
        assert_eq!(hash[0], 0xe3);
        assert_eq!(hash[1], 0xb0);
    }

    #[test]
    fn test_sha256_abc() {
        let mut sha = Sha256::new();
        sha.update(b"abc");
        let hash = sha.finish();
        // SHA256("abc") = ba7816bf8f01cfea...
        assert_eq!(hash[0], 0xba);
        assert_eq!(hash[1], 0x78);
    }

    #[test]
    fn test_sha256_long() {
        let mut sha = Sha256::new();
        let data = [0x61u8; 1000]; // 'a' * 1000
        sha.update(&data);
        let hash = sha.finish();
        assert_ne!(hash, [0u8; 32]);
    }
}