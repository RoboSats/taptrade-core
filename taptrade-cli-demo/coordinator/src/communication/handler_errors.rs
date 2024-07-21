#[derive(Debug)]
pub enum BondError {
	InvalidBond(String),
	BondNotFound,
	CoordinatorError(String),
}

#[derive(Debug)]
pub enum FetchOffersError {
	NoOffersAvailable,
	DatabaseError(String),
}

#[derive(Debug)]
pub enum FetchEscrowConfirmationError {
	NotFoundError,
	DatabaseError(String),
}

#[derive(Debug)]
pub enum RequestError {
	DatabaseError(String),
	NotConfirmedError,
	NotFoundError,
}
