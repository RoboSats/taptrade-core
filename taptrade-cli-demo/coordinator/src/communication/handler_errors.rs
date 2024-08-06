#[derive(Debug)]
pub enum BondError {
	InvalidBond(String),
	BondNotFound,
	CoordinatorError(String),
}

#[derive(Debug)]
pub enum FetchOffersError {
	NoOffersAvailable,
	Database(String),
}

#[derive(Debug)]
pub enum FetchEscrowConfirmationError {
	NotFound,
	Database(String),
}

#[derive(Debug)]
pub enum RequestError {
	Database(String),
	NotConfirmed,
	NotFound,
	PsbtAlreadySubmitted,
	PsbtInvalid(String),
}
