use super::*;

pub fn generate_random_order_id(len: usize) -> String {
	// Generate `len` random bytes
	let bytes: Vec<u8> = rand::thread_rng()
		.sample_iter(&rand::distributions::Standard)
		.take(len)
		.collect();

	// Convert bytes to hex string
	let hex_string = hex::encode(bytes);
	hex_string
}
