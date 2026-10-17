use crate::types::ProcessStatus;
use sysinfo::System;

/// Detect opencode / opencode-desktop running processes (case-insensitive, .exe stripped).
pub fn check() -> ProcessStatus {
    // Test / power-user escape hatch: assume opencode is offline.
    if std::env::var_os("OPENCODE_CLEANER_SKIP_PROC_CHECK").is_some() {
        return ProcessStatus {
            running: false,
            processes: vec![],
        };
    }
    let sys = System::new_all();

    let targets = ["opencode", "opencode-desktop"];
    let mut found = Vec::new();
    let my_pid = std::process::id();

    for (_, proc_) in sys.processes() {
        if proc_.pid().as_u32() == my_pid {
            continue;
        }
        let name = proc_.name().to_string_lossy().to_lowercase();
        let name = name.strip_suffix(".exe").unwrap_or(&name).to_string();
        // Also match on the executable file basename to catch bundled binaries
        let exe_name = proc_
            .exe()
            .and_then(|p| p.file_stem())
            .map(|s| s.to_string_lossy().to_lowercase())
            .unwrap_or_default();

        for t in targets {
            if name == t || exe_name == t {
                if !found.contains(&t.to_string()) {
                    found.push(t.to_string());
                }
            }
        }
    }

    ProcessStatus {
        running: !found.is_empty(),
        processes: found,
    }
}
