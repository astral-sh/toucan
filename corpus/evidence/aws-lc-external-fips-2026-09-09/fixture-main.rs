use aws_lc_rs::{aead, digest, hmac, rand, signature};
use signature::KeyPair;
use std::path::PathBuf;

fn hex(source: &str) -> Vec<u8> {
    assert_eq!(source.len() % 2, 0);
    source
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

fn record(name: &str, bytes: &[u8]) {
    let output =
        PathBuf::from(std::env::var_os("TOUCAN_CONSUMER_OUTPUT").expect("artifact directory"));
    std::fs::write(output.join(format!("{name}.bin")), bytes).unwrap();
}

fn hashes() {
    for (name, message, expected) in [
        (
            "sha256-empty",
            b"".as_slice(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        ),
        (
            "sha256-abc",
            b"abc".as_slice(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        ),
    ] {
        let value = digest::digest(&digest::SHA256, message);
        assert_eq!(value.as_ref(), hex(expected));
        record(name, value.as_ref());
    }
    for length in [0, 1, 55, 56, 63, 64, 65, 1024, 8191, 8192] {
        let message: Vec<u8> = (0..length).map(|n| (n * 31 + 7) as u8).collect();
        let expected = digest::digest(&digest::SHA256, &message);
        let mut context = digest::Context::new(&digest::SHA256);
        for chunk in message.chunks(17) {
            context.update(chunk);
        }
        assert_eq!(context.finish().as_ref(), expected.as_ref());
        record(&format!("sha256-{length}"), expected.as_ref());
    }
    // RFC 4231, test case 1.
    let key = hmac::Key::new(hmac::HMAC_SHA256, &[0x0b; 20]);
    let message = b"Hi There";
    let tag = hmac::sign(&key, message);
    assert_eq!(
        tag.as_ref(),
        hex("b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7")
    );
    hmac::verify(&key, message, tag.as_ref()).unwrap();
    let mut invalid = tag.as_ref().to_vec();
    invalid[0] ^= 1;
    assert!(hmac::verify(&key, message, &invalid).is_err());
    assert!(hmac::verify(&key, message, &tag.as_ref()[1..]).is_err());
    assert!(hmac::verify(&key, b"different", tag.as_ref()).is_err());
    record("hmac-sha256", tag.as_ref());
    println!(
        "SHA256: 2 known answers and 10 streaming lengths; HMAC: known answer and 3 rejections"
    );
}

fn encryption() {
    for (name, algorithm) in [
        ("aes128gcm", &aead::AES_128_GCM),
        ("aes256gcm", &aead::AES_256_GCM),
        ("chacha20poly1305", &aead::CHACHA20_POLY1305),
    ] {
        let key = aead::LessSafeKey::new(
            aead::UnboundKey::new(algorithm, &vec![0x42; algorithm.key_len()]).unwrap(),
        );
        for (index, length) in [0, 1, 15, 16, 17, 63, 64, 65, 1024].into_iter().enumerate() {
            let mut nonce = [0; 12];
            nonce[4..].copy_from_slice(&(index as u64).to_be_bytes());
            let plaintext: Vec<u8> = (0..length).map(|n| (n * 19 + index) as u8).collect();
            let aad = b"toucan native consumer";
            let mut ciphertext = plaintext.clone();
            key.seal_in_place_append_tag(
                aead::Nonce::assume_unique_for_key(nonce),
                aead::Aad::from(aad),
                &mut ciphertext,
            )
            .unwrap();
            let mut opened = ciphertext.clone();
            assert_eq!(
                key.open_in_place(
                    aead::Nonce::assume_unique_for_key(nonce),
                    aead::Aad::from(aad),
                    &mut opened
                )
                .unwrap(),
                plaintext
            );
            let mut corrupted = ciphertext.clone();
            *corrupted.last_mut().unwrap() ^= 1;
            assert!(key
                .open_in_place(
                    aead::Nonce::assume_unique_for_key(nonce),
                    aead::Aad::from(aad),
                    &mut corrupted
                )
                .is_err());
            let mut wrong_aad = ciphertext.clone();
            assert!(key
                .open_in_place(
                    aead::Nonce::assume_unique_for_key(nonce),
                    aead::Aad::from(b"wrong"),
                    &mut wrong_aad
                )
                .is_err());
            let mut wrong_nonce = nonce;
            wrong_nonce[0] ^= 1;
            let mut invalid = ciphertext.clone();
            assert!(key
                .open_in_place(
                    aead::Nonce::assume_unique_for_key(wrong_nonce),
                    aead::Aad::from(aad),
                    &mut invalid
                )
                .is_err());
            let mut short = ciphertext[..algorithm.tag_len() - 1].to_vec();
            assert!(key
                .open_in_place(
                    aead::Nonce::assume_unique_for_key(nonce),
                    aead::Aad::from(aad),
                    &mut short
                )
                .is_err());
            record(&format!("{name}-{length}"), &ciphertext);
        }
    }
    println!("AEAD: 3 algorithms, 27 round trips and 108 authentication/input rejections");
}

fn signatures() {
    // RFC 8032, Ed25519 test 1 (empty message).
    let seed = hex("9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60");
    let public = hex("d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a");
    let key = signature::Ed25519KeyPair::from_seed_and_public_key(&seed, &public).unwrap();
    let signed = key.sign(b"");
    assert_eq!(signed.as_ref(), hex("e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b"));
    let verifier = signature::UnparsedPublicKey::new(&signature::ED25519, &public);
    verifier.verify(b"", signed.as_ref()).unwrap();
    assert!(verifier.verify(b"modified", signed.as_ref()).is_err());
    let mut corrupted = signed.as_ref().to_vec();
    corrupted[0] ^= 1;
    assert!(verifier.verify(b"", &corrupted).is_err());
    assert!(verifier.verify(b"", &signed.as_ref()[1..]).is_err());
    let mut wrong_public = public.clone();
    wrong_public[0] ^= 1;
    assert!(signature::Ed25519KeyPair::from_seed_and_public_key(&seed, &wrong_public).is_err());
    record("ed25519-empty", signed.as_ref());

    let mut private = [0; 32];
    private[31] = 1;
    let public = hex("046b17d1f2e12c4247f8bce6e563a440f277037d812deb33a0f4a13945d898c2964fe342e2fe1a7f9b8ee7eb4a7c0f9e162bce33576b315ececbb6406837bf51f5");
    let key = signature::EcdsaKeyPair::from_private_key_and_public_key(
        &signature::ECDSA_P256_SHA256_ASN1_SIGNING,
        &private,
        &public,
    )
    .unwrap();
    let signed = key
        .sign(&rand::SystemRandom::new(), b"toucan native consumer")
        .unwrap();
    let verifier = signature::UnparsedPublicKey::new(
        &signature::ECDSA_P256_SHA256_ASN1,
        key.public_key().as_ref(),
    );
    verifier
        .verify(b"toucan native consumer", signed.as_ref())
        .unwrap();
    assert!(verifier.verify(b"modified", signed.as_ref()).is_err());
    assert!(verifier
        .verify(b"toucan native consumer", &signed.as_ref()[1..])
        .is_err());
    println!("Ed25519: known answer and 4 rejections; P256: signature round trip and 2 rejections");
}

fn main() {
    assert_eq!(unsafe { aws_lc_sys::BORINGSSL_integrity_test() }, 1);
    println!("FIPS integrity check: passed");
    hashes();
    encryption();
    signatures();
    for (name, size, alignment) in [
        (
            "SHA_CTX",
            std::mem::size_of::<aws_lc_sys::SHA_CTX>(),
            std::mem::align_of::<aws_lc_sys::SHA_CTX>(),
        ),
        (
            "SHA256_CTX",
            std::mem::size_of::<aws_lc_sys::SHA256_CTX>(),
            std::mem::align_of::<aws_lc_sys::SHA256_CTX>(),
        ),
        (
            "SHA512_CTX",
            std::mem::size_of::<aws_lc_sys::SHA512_CTX>(),
            std::mem::align_of::<aws_lc_sys::SHA512_CTX>(),
        ),
        (
            "EVP_AEAD_CTX",
            std::mem::size_of::<aws_lc_sys::EVP_AEAD_CTX>(),
            std::mem::align_of::<aws_lc_sys::EVP_AEAD_CTX>(),
        ),
        (
            "CBS",
            std::mem::size_of::<aws_lc_sys::CBS>(),
            std::mem::align_of::<aws_lc_sys::CBS>(),
        ),
        (
            "CBB",
            std::mem::size_of::<aws_lc_sys::CBB>(),
            std::mem::align_of::<aws_lc_sys::CBB>(),
        ),
    ] {
        println!("layout {name} {size} {alignment}");
    }
}
