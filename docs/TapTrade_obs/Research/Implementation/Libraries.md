## [BDK](https://docs.rs/bdk/latest/bdk/)

A modern, lightweight, descriptor-based wallet library written in Rust. 
Higher level, with wallet functionality. Supports Miniscript descriptors.

## [rust-bitcoin crate](https://docs.rs/bitcoin/latest/bitcoin/)

Lower level, lighter library for assembling transactions, generating addresses etc.

## [musig2 crate](https://docs.rs/musig2/latest/musig2/)

Still in beta but could be used by us.

This crate provides a flexible rust implementation of [MuSig2](https://eprint.iacr.org/2020/1261), an optimized digital signature aggregation protocol, on the `secp256k1` elliptic curve.

MuSig2 allows groups of mutually distrusting parties to cooperatively sign data and aggregate their signatures into a single aggregated signature which is indistinguishable from a signature made by a single private key. The group collectively controls an _aggregated public key_ which can only create signatures if everyone in the group cooperates (AKA an N-of-N multisignature scheme). MuSig2 is optimized to support secure signature aggregation with only **two round-trips of network communication.**