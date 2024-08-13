# *taptrade-core - A taproot p2p trading pipeline*

Implementation of a [taproot](https://bitcoinops.org/en/topics/taproot/) onchain p2p trading pipeline for fiat <-> bitcoin exchange with minimal onchain footprint. Intended for the integration in RoboSats and possibly other p2p exchanges.

## Background
Currently, all [RoboSats](https://github.com/RoboSats/robosats) trades use the Lightning network and utilize [HODL Invoices](https://bitcoinops.org/en/topics/hold-invoices/) to enable a safe exchange environment for users. In case of a dispute, e.g., the selling trader claims that the buyer did not transfer the fiat money, both parties can initiate a dispute process. In this process, the coordinator collects evidence (e.g., bank statements, chat logs) and releases the funds to the appropriate party.

This model works reliably for a large number of trades but has some tradeoffs compared to an on-chain exchange model nevertheless. Currently, the trading duration is limited to around 24 hours due to technical limitations regarding Lightning invoices (HTLC timeout) to keep the risk of technical complications like channel closes and bad UX trough payment failures low. Due to this limitation it is only possible to trade with fast fiat payment methods like SEPA Instant or PayPal. Longer trades would enable slower fiat rails like traditional bank wire which can take several days to arrive. The usage of the Lightning Network also limits the trade amount to a certain maximum due to nature of payment channels.

Therefore the implementation of a purely on-chain pipeline would enable larger trades and new fiat payment methods for traders. By utilizing the Taproot transaction format of Bitcoin and MuSig2 signature aggregation it is possible to design a trade pipeline which, in the regular case of no disputes, looks like a very regular Taproot transaction on the blockchain. This provides the traders with a high level of privacy and a lower fee rate than currently established P2WSH trade pipelines.

## Goals

## Architecture

### Trade pipeline
Insert protocol flow diagram

### Communication
HTTP Api

### Implementation
BDK+RustBitcoin+MuSig2+Axum+SQlite+Tokio+...

## Status

## Contribution

## Resources

<!-- ### Research
Find the current research as [Obsidian](https://obsidian.md/) formatted documents under /docs/TapTrade_obs.

### Implementation -->
<!-- TBD -->
