use crate::wallet::wallet_utils::get_seed;
use anyhow::{anyhow, Error, Result};
use musig2::{PubNonce, SecNonce, SecNonceBuilder};
use std::time::{SystemTime, UNIX_EPOCH};

// https://docs.rs/musig2/latest/musig2/

// secret nonce has to be used only one time!
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
