#[path="/home/dev-user/code/oss/toucan/fuzz/fuzz_targets/checked_invariants.rs"]
mod checked_invariants;
fn main() {
    let mut accepted=0;
    let mut rejected=0;
    let mut paths=std::fs::read_dir("/home/dev-user/code/oss/toucan/fuzz/seeds/checked").unwrap().map(|p|p.unwrap().path()).collect::<Vec<_>>();
    paths.sort();
    for path in paths {
        let source=std::fs::read_to_string(&path).unwrap();
        for profile in toucan::CompilerProfile::ALL.into_iter().flat_map(|p|toucan::LanguageMode::ALL.map(|m|p.with_language_mode(m))) {
            let mut config=toucan::Config::with_profile(profile);
            config.preprocessor.allow_filesystem=false;
            let normal=toucan::parse_source(&path,&source,&config);
            config.analysis.retain_code=true;
            let retained=toucan::parse_source(&path,&source,&config);
            match(normal,retained) {
                (Ok(a),Ok(b))=>{
                    assert_eq!(format!("{:?}",a.unit()),format!("{:?}",b.unit()),"{path:?} {profile:?}");
                    checked_invariants::check(b.analysis(),&b.preprocessed().source);
                    accepted+=1;
                }
                (Err(a),Err(b))=>{
                    assert_eq!(format!("{a:?}"),format!("{b:?}"),"{path:?} {profile:?}");
                    rejected+=1;
                }
                (a,b)=>panic!("{path:?} {profile:?}: {:?} {:?}",a.err(),b.err()),
            }
        }
    }
    println!("accepted={accepted} matching_diagnostics={rejected}");
}
