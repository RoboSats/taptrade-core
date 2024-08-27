/// This module provides utility functions for working with MuSig2 in the context of wallets.
///
/// The `MuSigData` struct represents the data required for MuSig2 signing. It contains the nonce,
/// public key, and secret key.
///
/// The `MusigNonce` struct represents the nonce used in MuSig2 signing. It contains the secret
/// nonce, as well as flags indicating whether it has been accessed for signing or sharing.
///
/// The `generate` function in the `MusigNonce` implementation generates a new `MusigNonce` with a
/// secret nonce based on the current timestamp.
///
/// The `get_sec_for_signing` function in the `MusigNonce` implementation returns the secret nonce
/// for signing, ensuring that it has not been accessed for signing before.
///
/// The `get_pub_for_sharing` function in the `MusigNonce` implementation returns the public nonce
/// for sharing, ensuring that the nonce has not been accessed for sharing or signing before.
///
/// The `create` function in the `MuSigData` implementation creates a new `MuSigData` instance
/// based on an extended private key and a secp256k1 context. It generates a new `MusigNonce` and
/// converts the keypair to the appropriate types for MuSig2 signing.
use crate::wallet::bitcoin::key::{Parity, Secp256k1, XOnlyPublicKey};
use crate::wallet::{wallet_utils::get_seed, KeychainKind};
use anyhow::{anyhow, Error, Result};
use bdk::bitcoin::secp256k1::PublicKey;
use bdk::{
	bitcoin::{bip32::ExtendedPrivKey, secp256k1::All},
	keys::{DescriptorPublicKey, DescriptorSecretKey},
	template::{Bip86, DescriptorTemplate},
};
use musig2::{secp256k1::SecretKey as MusigSecretKey, PubNonce, SecNonce, SecNonceBuilder};
use std::time::{SystemTime, UNIX_EPOCH};

// https://docs.rs/musig2/latest/musig2/

#[derive(Debug)]
pub struct MuSigData {
	pub nonce: MusigNonce,
	pub public_key: PublicKey,
	pub secret_key: MusigSecretKey,
}

// secret nonce has to be used only one time!
#[derive(Debug)]
pub struct MusigNonce {
	secret_nonce: SecNonce,
	accessed_for_signing: bool,
	accessed_for_sharing: bool,
}

impl MusigNonce {
	pub fn generate() -> Result<MusigNonce> {
		let timestamp_salt = SystemTime::now()
			.duration_since(UNIX_EPOCH)?
			.as_nanos()
			.to_le_bytes();

		// more salt can be added e.g. pubkey or secret key
		let secret_nonce = SecNonceBuilder::new(get_seed())
			.with_extra_input(&timestamp_salt)
			.build();
		Ok(MusigNonce {
			secret_nonce,
			accessed_for_sharing: false,
			accessed_for_signing: false,
		})
	}

	pub fn get_sec_for_signing(mut self) -> Result<SecNonce> {
		if self.accessed_for_signing {
			return Err(anyhow!("MuSig nonce has already been used for signing!"));
		}
		self.accessed_for_signing = true;
		Ok(self.secret_nonce)
	}

	pub fn get_pub_for_sharing(&mut self) -> Result<PubNonce> {
		if self.accessed_for_sharing || self.accessed_for_signing {
			return Err(anyhow!("MuSig nonce reused!"));
		}
		self.accessed_for_sharing = true;
		Ok(self.secret_nonce.public_nonce())
	}
}

impl MuSigData {
	pub fn create(xprv: &ExtendedPrivKey, secp_ctx: &Secp256k1<All>) -> Result<MuSigData> {
		let nonce = MusigNonce::generate()?;
		let keypair = xprv.to_owned().to_keypair(secp_ctx); // double check keypair, which derivation should we use?

		// convert from bdk secp to musig crate secp for the traits needed to do sig agg
		let musig_type_secret_key = MusigSecretKey::from_slice(&keypair.secret_bytes())?;

		Ok(MuSigData {
			nonce,
			public_key: keypair.public_key(),
			secret_key: musig_type_secret_key,
		})
	}
}
