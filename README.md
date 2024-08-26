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

## Trade protocol
#### <u>Bonds</u>
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

Example transaction [hex]:
```
0100000000010129a7c05b63693a5cb3ced62192929dc7074f988852301554cdb34e1aa04db1c90000000000feffffff02c56b029500000000225120ddce5be8a1713afb247da815f1545ccb997f593248e6b6d19e0e72a502baf6f28813000000000000225120fe8d8d8ff0985a33bc0b9c16508f8d4fe3d5d4df568a1fe6ad4c3022bbd185fb0140a448c42d65ad6a8febdae8db01d2ff388d8d746c2de84b0524ac9fc0d5af9c09a86c4f8da50ae91a993ce51ce99458bfcf9ba51f3da4aad4a43dd9ac6909cf1823180000
```

#### <u>Escrow locking transaction</u>
After a taker accepted a public offer (valid bond submitted) both traders have to lock funds in a locking transaction that can only be spent again in collaboration. This locking transaction is a collaborative transaction containing inputs of both maker and taker. Both traders have to sign the locking transaction
which is then combined by the coordinator. The coordinator will allow the use of the same inputs as in the bond transaction. Once this locking transaction has sufficient confirmations the fiat exchange can begin.

```
                  |---> Escrow locking output (will be used for the payout tx)
Maker input(s)----|---> Coordinator fee output (service fee for coordinator)
                  |---> Maker change output
Taker input(s)----|---> Taker change output
                  |---> Transaction fee

Buyer input amount:  Bond + 1/2 coordinator service fee + 1/2 tx fee
Seller input amount: Bond + 1/2 coordinator service fee + 1/2 tx fee + amount to sell
```

Example transaction [hex]:
```
01000000000102176c5c190fd523e6afeead69c04bff1269654500ffb87d67f880f08b6fe4f8cf0000000000feffffff1c97e5c22e16265cd971ba0b3ff1bf7ecf9359406547e0117d031b46215fbe0c0000000000feffffff046847019500000000225120f3ad0c624fa99f7430f63ead6343bf16508cb0d29baddd87f57bc2fb9dc2d6ce9026814a000000002251208c60833747870f8f94a947b63973ee8027615854432e5a7f9a420eccb3387f58b0ad010000000000225120dba8af662648dd045cea2986cf763e0971e034fdee96508043e0a4a9b1d27066d00700000000000022512027e02dc2cb93d385edc792b259598f52030e55cfc06d281418c95796bfc7789c01407f45bde83c7d2867d9b68f729738ad031fd9ff366337be3ba475743600362a74bd36c139cba460771804ea3a8ababb04cc1a478a3072e537f454bab239a1a2b2014045673c8327865d043d66bf59ab29a44ab4c24a1258b775244d435a4af1643b84479952e118b602b59d461806c7fee7706c5d1c3236866d0f5b2054acd28f718e22180000
```
#### <u>Escrow locking output</u>
The [taproot](https://github.com/bitcoin/bips/blob/master/bip-0341.mediawiki) escrow output can be unlocked in different ways depending on the trade outcome. In the
case of a successful trade the output can be spent using the [keypath](https://github.com/bitcoin/bips/blob/master/bip-0341.mediawiki#user-content-Taproot_key_path_spending_signature_validation) with an aggregated signature from two partial signatures of maker and taker, using the MuSig2 scheme. In the case of disputes different script paths can be used to enable unlocking by the coordinator in collaboration with one of the traders.
Other scripts can be added for edge cases, like a timeout script if the coordinator vanishes.
```
                     taproot output key
                              |
                              |
          --------------------------------------------
          |                                          |
          |                                          |
     internal key                              escrow spending scripts root
          =                                                 =
   Aggregated pubkey of            ---------------------          ---------------------
     Maker and Taker              | Maker + Coordinator |   +    | Taker + Coordinator |
                                   ---------------------          ---------------------
                                            and more potential useful scripts
```

The following script paths are currently implemented in the demonstrator:
```rust
// Maker wins escrow:
let policy_a_string = format!("and(pk({}),pk({}))", maker_pk, coordinator_pk);

// Taker wins escrow:
let policy_b_string = format!("and(pk({}),pk({}))", taker_pk, coordinator_pk);

// To prevent the possibility of extortion through the coordinator:
let policy_c_string = format!("and(pk({}),after(12228))", maker_pk);

// In case the coordinator vanishes or doesn't cooperate anymore,
// could be used with a cli toolkit as rescue method for traders.
let policy_d_string = format!("and(and(pk({}),pk({})),after(2048))", maker_pk, taker_pk);

// a fully assembled output descriptor would look like this (containing the XOnly pubkeys):
let escrow_output_descriptor = "tr(f00949d6dd1ce99a03f88a1a4f59117d553b0da51728bb7fd5b98fbf541337fb,{{and_v(v:pk(4987f3de20a9b1fa6f76c6758934953a8d615e415f1a656f0f6563694b53107d),pk(62333597c10487d959265bfc992514435daf74e26fd636f6b70e8936b4a82f3e)),and_v(v:pk(f1f1db08126af105974cde6021096525ed390cf9b7cde5fedb17a0b16ed31151),pk(62333597c10487d959265bfc992514435daf74e26fd636f6b70e8936b4a82f3e))},{and_v(v:and_v(v:pk(4987f3de20a9b1fa6f76c6758934953a8d615e415f1a656f0f6563694b53107d),pk(f1f1db08126af105974cde6021096525ed390cf9b7cde5fedb17a0b16ed31151)),after(2048)),and_v(v:pk(4987f3de20a9b1fa6f76c6758934953a8d615e415f1a656f0f6563694b53107d),after(12228))}})#wufuc530"
```

#### <u>Payout transaction</u>
Once the exchange has been completed, or a trader requested escrow, the escrow output will be spent again to complete the trade.
The transaction is assembled by the coordinator and shared with the clients for signing. Once the coordinator collected the
necessary signatures the transaction gets finalized and broadcasted.

**Example transaction**, both traders satisfied, keypath spend using aggregated signature (2-of-2):
```
                | --> bought amount + bond -> Buyer
escrow utxo --> | --> bond -> Seller
                | --> tx fee
(signed using
agg. sig for
keypath)

Example transaction [hex]: 01000000000101c544f644f6f31ca07dfa87a12aac0f103f3cf91483511f275ceeab316f6fa9c90200000000feffffff029808000000000000225120f610990da79b3bddd44a0820e31546319630c06221ea81bd3b798e4dfe9f5c6e388f0100000000002251201aa6d2c49082ae948e87ceb8551496cd6d951b093def3a3269d812db9e3808cf0140c0d07846e1b2b1deeca4a0cf35843417fefbe63086ff491ecc07638a099c0901138ac6e59a9e8b9a0878c098b206bee3427156dd0248d80de80fbdd8540ea00422180000
```

**Example transaction**, buyer sent fiat and won escrow, seller doesn't cooperate:
```
                | --> bought amount + buyer bond + seller bond -> Buyer
escrow utxo --> | --> tx fee

(signed using script path with buyer + coordinator signatures, could also use MuSig)
```

## Implementation

The protocol partly handled by a coordinator and partly by the client. The coordinator, running on the exchange side, handles trader matching, construction of transactions, monitoring of bonds and more tasks. The client could be bundled to a wasm library and included in the RoboSats frontend. Currently clients are only supposed to talk to the coordinator, not to each other.

Used libraries:
* [bdk](https://docs.rs/bdk/latest/bdk/) + [rust-bitcoin](https://docs.rs/bitcoin/latest/bitcoin/index.html): Transaction construction, signing, serialization, broadcasting
* [musig2](https://docs.rs/musig2/latest/musig2/): Signature aggregation
* [sqlx](https://docs.rs/sqlx/latest/sqlx/): Storing trade state in a sqlite database
* [axum](https://docs.rs/axum/latest/axum/): HTTP API (communication client <-> coordinator)


## Project Status
The repository contains a working CLI demonstrator that is able to complete the trade flow using the MuSig keypath spend on regtest or testnet. This demonstrator can be used to validate and experiment with the concept but is not intended for production use.

## Contribution
WIP
## Resources
WIP
<!-- ### Research
Find the current research as [Obsidian](https://obsidian.md/) formatted documents under /docs/TapTrade_obs.

### Implementation -->
<!-- TBD -->
