#[path = "/home/dev-user/.codex/worktrees/toucan-vector-static/fuzz/fuzz_targets/checked_invariants.rs"]
mod checked_invariants;
fn main() {
    let mut accepted = 0;
    let mut rejected = 0;
    let mut paths = std::fs::read_dir("/home/dev-user/.codex/worktrees/toucan-vector-static/fuzz/seeds/checked")
        .unwrap().map(|entry| entry.unwrap().path()).collect::<Vec<_>>();
    paths.sort();
    for path in paths {
        let source = std::fs::read_to_string(&path).unwrap();
        for profile in toucan::CompilerProfile::ALL.into_iter().flat_map(|p| [toucan::LanguageMode::Gnu11, toucan::LanguageMode::C11].map(|m| p.with_language_mode(m))) {
            let options = toucan::semantic::AnalysisOptions { retain_code: true, ..Default::default() };
            match (toucan::semantic::analyze_with_profile(&source, profile, &Default::default()), toucan::semantic::analyze_with_profile(&source, profile, &options)) {
                (Ok(a), Ok(b)) => { assert_eq!(format!("{:?}", a.unit()), format!("{:?}", b.unit())); checked_invariants::check(&b, &source); accepted += 1; }
                (Err(a), Err(b)) => { assert_eq!((a.offset, a.message), (b.offset, b.message)); rejected += 1; }
                (a, b) => panic!("{path:?} {profile:?}: {a:?} {b:?}"),
            }
        }
    }
    let mut queries = 0;
    for profile in toucan::CompilerProfile::ALL.into_iter().flat_map(|p| [toucan::LanguageMode::Gnu11, toucan::LanguageMode::C11].map(|m| p.with_language_mode(m))) {
        let source = "typedef int I __attribute__((vector_size(16))); typedef float F __attribute__((vector_size(16))); typedef unsigned char B __attribute__((vector_size(16))); typedef I A __attribute__((aligned(32)));";
        let unit = toucan::semantic::analyze_with_profile(source, profile, &Default::default()).unwrap().into_unit();
        for expression in ["(I){1,2,3,4}", "(I){1,2,3,4}+(I){5,6,7,8}", "(I){1,2,3,4}<(I){4,3,2,1}", "(I){-1,-2,3,4}>>2L", "__builtin_convertvector((I){1,-2,16777217,0},F)", "__builtin_convertvector((I){1,2,3,4},const A)", "(__typeof__(A)){1,2,3,4}"] {
            let value = toucan::semantic::evaluate_vector(&unit, expression).unwrap();
            assert_eq!(value.ty().lane_count(), 4);
            assert_eq!(value.lanes().len(), 4);
            queries += 1;
        }
        for expression in ["(I){1}/(I){0}", "(B){1}<<256", "-(I){-2147483648}", "(I){1,2,3,4,5}", "__builtin_convertvector((F){0x1p31f},I)"] {
            assert!(toucan::semantic::evaluate_vector(&unit, expression).is_err());
            queries += 1;
        }
        let value = toucan::semantic::evaluate_vector(&unit, "(__typeof__(A)){1,2,3,4}").unwrap();
        drop(unit);
        assert_eq!(value.ty().alignment_bytes(), 32);
    }
    println!("accepted={accepted} rejected={rejected} public_queries={queries}");
}
