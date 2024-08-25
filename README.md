# *taptrade-core - A taproot p2p trading pipeline*

Implementation of a [taproot](https://bitcoinops.org/en/topics/taproot/) escrow trading pipeline for fiat <-> bitcoin exchange with minimal onchain footprint. Intended for the integration in RoboSats and possibly other p2p exchanges.

## Background
Currently, all [RoboSats](https://github.com/RoboSats/robosats) trades use the [Lightning network](https://lightning.network/) and utilize [HODL Invoices](https://bitcoinops.org/en/topics/hold-invoices/) to enable a safe exchange environment for users. In case of a dispute, e.g., the (bitcoin) selling trader claims that the buyer did not transfer the fiat money, both parties can initiate a dispute process. In this process, the coordinator collects evidence (e.g. bank statements, chat logs) and releases the funds to the appropriate party.

This model works reliably for a large number of trades but has some tradeoffs compared to an on-chain exchange model nevertheless. Currently, the trading duration is limited to around 24 hours due to technical limitations regarding Lightning payments ([HTLC](https://docs.lightning.engineering/the-lightning-network/multihop-payments/hash-time-lock-contract-htlc) timeout) to keep the risk of technical complications like [channel closes](https://docs.lightning.engineering/the-lightning-network/payment-channels/lifecycle-of-a-payment-channel) and bad UX trough [payment failures](https://thebitcoinmanual.com/articles/why-lightning-payments-may-fail/) low. Due to this limitation it is only possible to trade with fast fiat payment methods like SEPA Instant or PayPal. Longer trades would allow for slower fiat rails like traditional bank wire which can take several days to arrive. The usage of the Lightning Network also limits the trade amount to a certain maximum due to nature of payment channels.

Therefore the implementation of a purely on-chain pipeline would enable larger trades and new fiat payment methods for traders. By utilizing the Taproot transaction format of Bitcoin and the [MuSig2](https://eprint.iacr.org/2020/1261) signature aggregation scheme it is possible to design a trade pipeline which, in the regular case of no disputes, looks like a very regular Taproot transaction on the blockchain. This provides the traders with a high level of privacy and a lower fee rate than currently established P2WSH trade pipelines.

## Goals
1. Definition of the trading protocol
2. Implementation of client and coordinator
3. Integration of the client in RoboSats frontend

## Architecture

#### Bonds
Traders are required to submit a bond to the coordinator as first step of their trade. This bond is required to prevent misbehaviour like orderbook spamming or unreliable trade partners, it establishes a real cost to create offers and not finish them as agreed upon. 

The bond is sent to the coordinator in form of a signed bitcoin transaction spending to the coordinator with a high transaction fee. The required bond amount can be communicated by the coordinator. The input to the bond transaction should be the same input that will be used in the following escrow locking transaction to reduce the risk of a griefing coordinator. 
It can also be required that the input sum should be at least the amount of the trade for sellers so there
is evidence the seller actually owns the bitcoin he wants to sell, increasing cost to fake offers.
The bond will be stored by the coordinator and the coordinator is supposed to monitor the mempool and the 
blockchain for the used inputs. 

In case the trader misbehaves the coordinator can broadcast the bond transaction and receives the bond output.
If the trader double spends the input to the bond the coordinator is able to increase the transaction fee ([CPFP](https://bitcoinops.org/en/topics/cpfp/)) up to the amount of the bond output. Even in the case 
of [out-of-band mining](https://thebitcoinmanual.com/articles/out-of-band-btc-transaction/) of the bond input
in another transacion the trader has a cost associated to creating offers on the exchange. 

This bond mechanism should be sufficient for the associated risks as long as out-of-band mining doesn't get extremely cheap.

```
Bond TX, signed, not broadcasted:

                  |-----> Bond output (coordinator)
Trader input(s)-->|-----> Change output
                  |-----> Tx fee (high)
```

#### Escrow locking transaction
After a taker accepted a public offer (valid bond submitted) both traders have to lock funds in a locking transaction that can only be spent again in collaboration. This locking transaction is a collaborative transaction containing inputs of both maker and taker. Both traders have to sign the locking transaction 
which is then combined by the coordinator. The coordinator will allow the use of the same inputs as in the bond transaction. Once this locking transaction has sufficient confirmations the fiat exchange can begin.

```
                  |---> Escrow locking output (will be used for the payout tx)
Maker input(s)----|---> Coordinator fee output (service fee for coordinator)
                  |---> Maker change output
Taker input(s)----|---> Taker change output
                  |---> Transaction fee
```
#### Escrow locking output
The taproot escrow output can be unlocked in different ways depending on the trade outcome. In the
happy case of a successful trade the output can be spent using the keyspend path with an aggregated signature two partial signatures from maker and taker, using the Musig2 scheme. In the case of disputes different script paths can be used to enable unlocking by the coordinator in collaboration with one of the traders. 
Other scripts can be added for edge cases, like a timeout script if the coordinator vanishes.
```
                     taproot output key
                              |
                              |
          --------------------------------------------
          |                                          |
          |                                          |
     internal key                              Escrow spending scripts
  Aggregated pubkey of                Maker + Coordinator        Taker + Coordinator
    Maker and Taker   
```

## Trade protocol  
WIP

## Implementation
WIP

BDK+RustBitcoin+MuSig2+Axum+SQlite+Tokio+...

The protocol partly handled by a coordinator and partly by the client. The coordinator, running on the exchange side, handles trader matching, construction of transactions, monitoring of bonds and more tasks. The client could be bundled to a wasm library and included in the RoboSats frontend. Currently clients are only supposed to talk to the coordinator, not to each other.

## Status
WIP
## Contribution
WIP
## Resources
WIP
<!-- ### Research
Find the current research as [Obsidian](https://obsidian.md/) formatted documents under /docs/TapTrade_obs.

### Implementation -->
<!-- TBD -->
