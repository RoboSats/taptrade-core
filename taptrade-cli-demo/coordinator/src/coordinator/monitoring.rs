// we create an async function that loops trough the sqlite db table active_maker_offers and
// continoously verifies the bond inputs (mempool and chain), maybe with some caching in a hashmap to
// prevent querying the db all the time.
// Also needs to implement punishment logic in case a fraud is detected.
