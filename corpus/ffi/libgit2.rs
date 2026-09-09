unsafe extern "C" {
    fn toucan_git_commit_allow_empty(
        options: *const b::git_commit_create_options,
    ) -> core::ffi::c_uint;
    fn toucan_git_commit_set_allow_empty(
        options: *mut b::git_commit_create_options,
        value: core::ffi::c_uint,
    );
}

fn ffi_test() {
    unsafe {
        assert!(b::git_libgit2_init() > 0);
        let (mut major, mut minor, mut revision) = (0, 0, 0);
        b::git_libgit2_version(&mut major, &mut minor, &mut revision);
        assert_eq!((major, minor, revision), (1, 9, 1));
        let hex = c"0123456789abcdef0123456789abcdef01234567";
        let mut oid = std::mem::MaybeUninit::<b::git_oid>::uninit();
        assert_eq!(b::git_oid_fromstr(oid.as_mut_ptr(), hex.as_ptr()), 0);
        let oid = oid.assume_init();
        let mut output = [0 as core::ffi::c_char; 41];
        assert!(
            !b::git_oid_tostr(output.as_mut_ptr(), output.len().try_into().unwrap(), &oid)
                .is_null()
        );
        assert_eq!(
            std::ffi::CStr::from_ptr(output.as_ptr()).to_bytes(),
            hex.to_bytes()
        );
        assert_eq!(b::git_oid_equal(&oid, &oid), 1);
        let mut signature = std::ptr::null_mut();
        assert_eq!(
            b::git_signature_new(
                &mut signature,
                c"Toucan".as_ptr(),
                c"toucan@example.invalid".as_ptr(),
                1_700_000_000,
                0
            ),
            0
        );
        assert_eq!(
            std::ffi::CStr::from_ptr((*signature).name).to_bytes(),
            b"Toucan"
        );
        assert_eq!((*signature).when.time, 1_700_000_000);
        let mut options =
            std::mem::MaybeUninit::<b::git_commit_create_options>::zeroed().assume_init();
        options.version = b::GIT_COMMIT_CREATE_OPTIONS_VERSION.try_into().unwrap();
        options.author = signature;
        assert_eq!(options.allow_empty_commit(), 0);
        toucan_git_commit_set_allow_empty(&mut options, 1);
        assert_eq!(options.allow_empty_commit(), 1);
        options.set_allow_empty_commit(0);
        assert_eq!(toucan_git_commit_allow_empty(&options), 0);
        options.set_allow_empty_commit(1);
        assert_eq!(toucan_git_commit_allow_empty(&options), 1);
        assert_eq!(options.version, 1);
        assert_eq!(options.author, signature);
        b::git_signature_free(signature);
        assert_eq!(b::git_libgit2_shutdown(), 0);
    }
}
