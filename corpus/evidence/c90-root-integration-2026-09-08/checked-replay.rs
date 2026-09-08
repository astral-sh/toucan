#[path = "/home/dev-user/code/oss/toucan/fuzz/fuzz_targets/checked_invariants.rs"]
mod checked_invariants;
fn main() {
    let mut accepted = 0;
    let mut rejected = 0;
    let mut paths = std::fs::read_dir("/home/dev-user/code/oss/toucan/fuzz/seeds/checked")
        .unwrap().map(|entry| entry.unwrap().path()).collect::<Vec<_>>();
    paths.sort();
    for path in paths {
        let source = std::fs::read_to_string(&path).unwrap();
        for profile in toucan::CompilerProfile::ALL.into_iter().flat_map(|p| toucan::LanguageMode::ALL.map(|m| p.with_language_mode(m))) {
            let options = toucan::semantic::AnalysisOptions { retain_code: true, ..Default::default() };
            match (toucan::semantic::analyze_with_profile(&source, profile, &Default::default()), toucan::semantic::analyze_with_profile(&source, profile, &options)) {
                (Ok(a), Ok(b)) => { assert_eq!(format!("{:?}", a.unit()), format!("{:?}", b.unit())); checked_invariants::check(&b, &source); accepted += 1; }
                (Err(a), Err(b)) => { assert_eq!((a.offset, a.message), (b.offset, b.message)); rejected += 1; }
                (a, b) => panic!("{path:?} {profile:?}: {a:?} {b:?}"),
            }
        }
    }
    println!("accepted={accepted} rejected={rejected}");
}
