use super::*;

pub enum EscrowWinner {
	Maker,
	Taker,
}

#[derive(Clone, Debug)]
pub struct EscrowCase {
	pub maker_id: String,
	pub taker_id: String,
	pub offer_id: String,
}

fn get_coordinator_cli_input(escrow_case: EscrowCase) -> EscrowWinner {
	let cli_prompt = format!(
		"\n\nMaker: {}\nTaker: {}\nOffer: {}\nare in dispute. Who won? Enter M for Maker, T for Taker",
		escrow_case.maker_id, escrow_case.taker_id, escrow_case.offer_id
	);
	loop {
		LOGGING_ENABLED.store(false, Ordering::Relaxed);
		println!("{}", cli_prompt);
		let mut input = String::new();
		std::io::stdin().read_line(&mut input).unwrap();
		LOGGING_ENABLED.store(true, Ordering::Relaxed);
		match input.trim() {
			"M" => return EscrowWinner::Maker,
			"T" => return EscrowWinner::Taker,
			_ => println!("Invalid input, please enter M or T"),
		};
	}
}

pub async fn escrow_cli_loop(database: Arc<CoordinatorDB>) {
	loop {
		let open_escrows: Vec<EscrowCase> = database
			.get_open_escrows()
			.await
			.expect("Database failure, cannot fetch escrow cases");

		for escrow in open_escrows {
			let escrow_clone = escrow.clone();
			let escrow_input_result =
				tokio::task::spawn_blocking(move || get_coordinator_cli_input(escrow_clone)).await;

			match escrow_input_result {
				Ok(EscrowWinner::Maker) => {
					database
						.resolve_escrow(&escrow.offer_id, &escrow.maker_id)
						.await
						.expect("Database failure, cannot resolve escrow. Restart coordinator.");
				}
				Ok(EscrowWinner::Taker) => {
					database
						.resolve_escrow(&escrow.offer_id, &escrow.taker_id)
						.await
						.expect("Database failure, cannot resolve escrow. Restart coordinator.");
				}
				_ => error!("Escrow resolving cli input error"),
			}
		}
		tokio::time::sleep(std::time::Duration::from_secs(5)).await;
	}
}
