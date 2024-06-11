use rand_core::{OsRng, RngCore};

// uses operating system rng which is secure for cryptography
pub fn get_seed() -> [u8; 32] {
	let mut seed = [0u8; 32];
	OsRng.fill_bytes(&mut seed);
	seed
}
