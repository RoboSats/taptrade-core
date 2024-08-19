Thinks to improve when implementing the production ready library/coordinator:
* secure user authentification scheme for calls / unique trade ids
* make api more generic (smaller) / maybe use websockets
* make bond punishment logic more advanced (raising fees, quicker monitoring, redundant mempools?)
* review escrow output descriptor, maybe make it smaller(less specific cases, more generic)?
* maybe hard code descriptor instead of compiling it from pieces?
* review for security flaws (error handling, logic bugs, crypto bugs)
* maybe switch wallet completely to core rpc instead of bdk wallet + core rpc
* api rate limiting (e.g. backoff) ?
* build trader toolkit to get funds out of escrow if coordinator dissapears
* use the same database as the existing (python) robosats coordinator instead of separate sqlite db?
* share single db for coordinator and bdk wallet instead of sqlite + bdk k/v db?
* add more test coverage
* move as much logic as possible (without safety tradeoffs) to coordinator side to make client lean
* update BDK to 1.0 once out of alpha
