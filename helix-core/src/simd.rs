/// Reports which SIMD code path was compiled.
#[must_use]
pub fn simd_path() -> &'static str {
    #[cfg(all(target_arch = "x86_64", target_feature = "avx2"))]
    {
        "avx2"
    }

    #[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
    {
        "neon"
    }

    #[cfg(not(any(
        all(target_arch = "x86_64", target_feature = "avx2"),
        all(target_arch = "aarch64", target_feature = "neon")
    )))]
    {
        "scalar"
    }
}
