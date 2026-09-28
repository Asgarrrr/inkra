//! BM25 with Lucene's `k1` and `b`: a term's rarity times its saturated, length-normalized count.

const K1: f64 = 1.2;
const B: f64 = 0.75;

/// The rarity of a term `df` of `n` notes hold; never negative while `df <= n`.
pub(crate) fn idf(df: u32, n: u32) -> f64 {
    let (df, n) = (f64::from(df), f64::from(n));
    (1.0 + (n - df + 0.5) / (df + 0.5)).ln()
}

/// A term's weight in a note of `len` words holding it `tf` times, where `avg` is the mean `len`.
pub(crate) fn bm25(idf: f64, tf: u32, len: u32, avg: f64) -> f64 {
    let tf = f64::from(tf);
    idf * tf * (K1 + 1.0) / (tf + K1 * (1.0 - B + B * f64::from(len) / avg))
}

#[cfg(test)]
mod tests;
