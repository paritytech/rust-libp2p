// Copyright 2017-2018 Parity Technologies (UK) Ltd.
//
// Permission is hereby granted, free of charge, to any person obtaining a
// copy of this software and associated documentation files (the "Software"),
// to deal in the Software without restriction, including without limitation
// the rights to use, copy, modify, merge, publish, distribute, sublicense,
// and/or sell copies of the Software, and to permit persons to whom the
// Software is furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS
// OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
// FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
// DEALINGS IN THE SOFTWARE.

//! Certificate handling for libp2p
//!
//! This handles generation, signing, and verification.
//!
//! This crate uses the `log` crate to emit log output.  Events that will occur normally are output
//! at `trace` level, while “expected” error conditions (ones that can result during correct use of the
//! library) are logged at `debug` level.

use libp2p_core::identity;
const LIBP2P_OID: &[u64] = &[1, 3, 6, 1, 4, 1, 53594, 1, 1];
pub(super) const LIBP2P_OID_BYTES: &[u8] = &[43, 6, 1, 4, 1, 131, 162, 90, 1, 1];
pub(super) const LIBP2P_SIGNING_PREFIX: [u8; 21] = *b"libp2p-tls-handshake:";
pub(super) const LIBP2P_SIGNING_PREFIX_LENGTH: usize = LIBP2P_SIGNING_PREFIX.len();
const LIBP2P_SIGNATURE_ALGORITHM_PUBLIC_KEY_LENGTH: usize = 65;
static LIBP2P_SIGNATURE_ALGORITHM: &rcgen::SignatureAlgorithm = &rcgen::PKCS_ECDSA_P256_SHA256;

fn encode_signed_key(public_key: identity::PublicKey, signature: &[u8]) -> rcgen::CustomExtension {
    let public_key = public_key.into_protobuf_encoding();
    let contents = yasna::construct_der(|writer| {
        writer.write_sequence(|writer| {
            writer
                .next()
                .write_bitvec_bytes(&public_key, public_key.len() * 8);
            writer
                .next()
                .write_bitvec_bytes(signature, signature.len() * 8);
        })
    });
    let mut ext = rcgen::CustomExtension::from_oid_content(LIBP2P_OID, contents);
    ext.set_criticality(true);
    ext
}

fn gen_signed_keypair(keypair: &identity::Keypair) -> (rcgen::KeyPair, rcgen::CustomExtension) {
    let temp_keypair = rcgen::KeyPair::generate(&LIBP2P_SIGNATURE_ALGORITHM)
        .expect("we pass valid parameters, and assume we have enough memory and randomness; qed");
    let mut signing_buf =
        [0u8; LIBP2P_SIGNING_PREFIX_LENGTH + LIBP2P_SIGNATURE_ALGORITHM_PUBLIC_KEY_LENGTH];
    let public = temp_keypair.public_key_raw();
    assert_eq!(
        public.len(),
        LIBP2P_SIGNATURE_ALGORITHM_PUBLIC_KEY_LENGTH,
        "ECDSA public keys are 65 bytes"
    );
    signing_buf[..LIBP2P_SIGNING_PREFIX_LENGTH].copy_from_slice(&LIBP2P_SIGNING_PREFIX[..]);
    signing_buf[LIBP2P_SIGNING_PREFIX_LENGTH..].copy_from_slice(public);
    let signature = keypair.sign(&signing_buf).expect("signing failed");
    (
        temp_keypair,
        encode_signed_key(keypair.public(), &signature),
    )
}

/// Generates a self-signed TLS certificate that includes a libp2p-specific certificate extension
/// containing the public key of the given keypair.
pub(crate) fn make_cert(keypair: &identity::Keypair) -> rcgen::Certificate {
    let mut params = rcgen::CertificateParams::new(vec![]);
    let (cert_keypair, libp2p_extension) = gen_signed_keypair(keypair);
    params.custom_extensions.push(libp2p_extension);
    params.alg = &LIBP2P_SIGNATURE_ALGORITHM;
    params.key_pair = Some(cert_keypair);
    rcgen::Certificate::from_params(params)
        .expect("certificate generation with valid params will succeed; qed")
}

/// Extracts the `PeerId` from a certificate’s libp2p extension. It is erroneous
/// to call this unless the certificate is known to be a well-formed X.509
/// certificate with a valid libp2p extension. The certificate verifiers in this
/// crate validate check this.
///
/// # Panics
///
/// Panics if there is no libp2p extension in the certificate, or if the
/// certificate is ill-formed.
pub(crate) fn unwrap_libp2p_certificate(certificate: &[u8]) -> libp2p_core::PeerId {
    let mut id = None;
    let cb = &mut |oid: untrusted::Input<'_>, value, _, _| match oid.as_slice_less_safe() {
        LIBP2P_OID_BYTES => {
            id = Some(extract_libp2p_peerid(value));
            webpki::Understood::Yes
        }
        _ => webpki::Understood::No,
    };
    webpki::EndEntityCert::from_with_extension_cb(certificate, cb)
        .expect("we already validated the certificate is well-formed");
    id.expect("we already checked that a libp2p extension exists")
}

fn extract_libp2p_peerid(extension: untrusted::Input<'_>) -> libp2p_core::PeerId {
    use ring::{error::Unspecified, io::der};
    extension
        .read_all(Unspecified, |mut reader| {
            let inner = der::expect_tag_and_get_value(&mut reader, der::Tag::Sequence)?;
            inner.read_all(Unspecified, |mut reader| {
                let public_key =
                    der::bit_string_with_no_unused_bits(&mut reader)?.as_slice_less_safe();
                der::bit_string_with_no_unused_bits(&mut reader)?;
                Ok(identity::PublicKey::from_protobuf_encoding(public_key)
                    .expect("already checked")
                    .into())
            })
        })
        .expect("we already checked this in the certificate verifier")
}
