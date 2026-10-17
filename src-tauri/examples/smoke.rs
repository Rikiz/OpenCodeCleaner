use opencode_cleaner_lib::{paths, scan};
fn main() {
    let p = paths::resolve().expect("resolve");
    println!("data_root: {}", p.data_root.display());
    let r = scan::scan(&p, |_| ()).expect("scan");
    println!("total: {:.1} MB", r.total_mb);
    println!("sessions: {}", r.sessions.len());
    println!("opencode_version: {:?}", r.opencode_version);
    for d in &r.directories { println!("  {:<14} {}", d.label, d.size_bytes); }
    println!("top 3 sessions:");
    for s in r.sessions.iter().take(3) {
        println!("  {:<32} {:.2} MB  parts={}", s.title, s.size_bytes as f64/1_048_576.0, s.part_count);
    }
    // dry-run: keep recent 50 + all dirs
    use opencode_cleaner_lib::types::{CleanSelection, DirKeySel};
    let sel = CleanSelection {
        session_ids: vec![], keep_recent: Some(50), older_than_days: None,
        min_size_mb: None, project_ids: vec![],
        directories: vec![DirKeySel::Log, DirKeySel::Snapshot, DirKeySel::CachePackages, DirKeySel::CacheBin],
    };
    let pv = opencode_cleaner_lib::clean::dry_run(&p, &sel).expect("dry_run");
    println!("\ndry_run: freed {:.2} MB, sessions {}", pv.total_freed_mb, pv.affected_session_count);
}
