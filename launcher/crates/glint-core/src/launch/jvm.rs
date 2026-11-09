use crate::instance::GcPreset;

/// Heap settings. The initial heap is capped so the game doesn't reserve the whole
/// allocation before it needs it.
pub fn memory_args(memory_mb: u32) -> Vec<String> {
    vec![
        format!("-Xms{}M", memory_mb.min(2048)),
        format!("-Xmx{memory_mb}M"),
    ]
}

/// Garbage collector flags for a client: short, predictable pauses over throughput.
pub fn gc_args(gc: GcPreset, java_major: u32) -> Vec<String> {
    let flags: &[&str] = match gc {
        // ZGC is only production-ready and generational from Java 21. On 21 and 22
        // generational mode is opt-in; from 23 it's the default and the flag is deprecated.
        GcPreset::Zgc if java_major >= 23 => &["-XX:+UseZGC"],
        GcPreset::Zgc if java_major >= 21 => &["-XX:+UseZGC", "-XX:+ZGenerational"],
        // The flags Mojang's launcher has shipped for years: a larger young generation
        // and a 50 ms pause target keep collections short enough not to drop frames.
        _ => &[
            "-XX:+UnlockExperimentalVMOptions",
            "-XX:+UseG1GC",
            "-XX:G1NewSizePercent=20",
            "-XX:G1ReservePercent=20",
            "-XX:MaxGCPauseMillis=50",
            "-XX:G1HeapRegionSize=32M",
        ],
    };
    flags.iter().map(|f| (*f).to_string()).collect()
}

/// Split a free-form argument string on whitespace, honouring double quotes so paths
/// with spaces survive (`-Dfoo="C:\Program Files\x"`).
pub fn split_args(input: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut has_token = false;
    for c in input.chars() {
        match c {
            '"' => {
                in_quotes = !in_quotes;
                has_token = true;
            }
            c if c.is_whitespace() && !in_quotes => {
                if has_token {
                    args.push(std::mem::take(&mut current));
                    has_token = false;
                }
            }
            c => {
                current.push(c);
                has_token = true;
            }
        }
    }
    if has_token {
        args.push(current);
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory() {
        assert_eq!(memory_args(4096), ["-Xms2048M", "-Xmx4096M"]);
        assert_eq!(memory_args(1024), ["-Xms1024M", "-Xmx1024M"]);
    }

    #[test]
    fn gc_by_java_version() {
        assert!(gc_args(GcPreset::G1, 25).contains(&"-XX:+UseG1GC".to_string()));
        assert_eq!(gc_args(GcPreset::Zgc, 25), ["-XX:+UseZGC"]);
        assert_eq!(
            gc_args(GcPreset::Zgc, 21),
            ["-XX:+UseZGC", "-XX:+ZGenerational"]
        );
        // Too old for generational ZGC: fall back to G1.
        assert!(gc_args(GcPreset::Zgc, 17).contains(&"-XX:+UseG1GC".to_string()));
    }

    #[test]
    fn splitting() {
        assert_eq!(split_args("  -Da=1   -Db=2 "), ["-Da=1", "-Db=2"]);
        assert_eq!(
            split_args(r#"-Dpath="C:\Program Files\x" -Xss2M"#),
            [r"-Dpath=C:\Program Files\x", "-Xss2M"]
        );
        assert!(split_args("   ").is_empty());
        assert_eq!(split_args(r#""""#), [""]);
    }
}
