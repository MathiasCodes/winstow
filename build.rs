fn main() {
    // Statically link the Visual C++ runtime on Windows to avoid requiring
    // vcredist2022 installation. This uses "hybrid linking" which statically
    // links vcruntime while dynamically linking the Universal C Runtime (ucrt)
    // that ships with Windows.
    #[cfg(windows)]
    static_vcruntime::metabuild();
}
