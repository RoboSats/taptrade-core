use rand_core::{RngCore, OsRng};

// uses operating system rng which is secure for cryptography
pub fn get_seed() -> [u8; 32] {
	let mut key = [0u8; 32];
	OsRng.fill_bytes(&mut key);
	key
}
