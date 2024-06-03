### <u>Relevant BIPs</u>

Understanding the following BIPs is relevant for the project.
#### BIP documents

* [BIP 340 - Schnorr Signatures for secp256k1](https://github.com/bitcoin/bips/blob/master/bip-0340.mediawiki)
* [BIP 341 - SegWit version 1 spending rules](https://github.com/bitcoin/bips/blob/master/bip-0341.mediawiki)
* [BIP 342 - Validation of Taproot Scripts](https://github.com/bitcoin/bips/blob/master/bip-0342.mediawiki)
* [BIP 114(Rejected but interesting) - Merkelized Abstract Syntax Tree](https://github.com/bitcoin/bips/blob/master/bip-0114.mediawiki)
* [BIP 65 - OP_CHECKLOCKTIMEVERIFY](https://github.com/bitcoin/bips/blob/master/bip-0065.mediawiki)
* [BIP 174 - Partially Signed Bitcoin Transaction Format](https://github.com/bitcoin/bips/blob/master/bip-0174.mediawiki)
* [BIP 370 - PSBT Version 2](https://github.com/bitcoin/bips/blob/master/bip-0370.mediawiki)
#### BIP Readtrough videos

* [BIP 340 Readtrough - Jimmy Song](https://www.youtube.com/watch?v=rVsNFMzQUck)
* [BIP 341 Readtrough - Jimmy Song](https://www.youtube.com/watch?v=pkS9aorpxNc)
* [BIP 342 Readtrough - Jimmy Song](https://www.youtube.com/watch?v=fAEcXh6nZ9U)

### <u>Taproot</u>

![[Taproot output structure.canvas]]

#### [[Spending Taproot UTXOs]]
#### Videos 

* [Introduction to Taproot - Nicolas Dorier - short video](https://www.youtube.com/watch?v=I7HsXKgtD2I)
* [Very good, practical introduction to coding taproot Transactions - Bitdevs LA](https://www.youtube.com/watch?v=E-HxgNkPB-8) 

#### Workshop
[Bitcoin Optech Schnorr Taproot Workshop](https://bitcoinops.org/en/schorr-taproot-workshop/)

### <u>Descriptors/Miniscript</u>

We can use Descriptors/Miniscript to precisely and human readable describe the Transaction outputs of the escrow transaction pipeline. This improves readability and portability of the pipeline.
#### Videos

* [Using BDK and Wallet descriptors for Taproot - Video - Bitdevs LA](https://www.youtube.com/watch?v=wsQIZRY2BD0)
* [Introduction to Miniscript - Port of Bitcoin](https://www.youtube.com/watch?v=uNZpfHEtP4U)
* [Getting started with Miniscript - Andrew Poelstra](https://www.youtube.com/watch?v=eTUuwASdUBE)
#### Websites

* [Miniscript introduction/compiler/specification](https://bitcoin.sipa.be/miniscript/)
* [https://bitcoinops.org/en/preparing-for-taproot/#taproot-descriptors](https://bitcoinops.org/en/preparing-for-taproot/#taproot-descriptors)


### <u>Signature/Pubkey aggregation</u>

To combine multiple parties pubkeys to a single combined Taproot pubkey and to create a aggregated signature with all participants MuSig2 is relevant.

* [Paper, very (probably too) deep](https://eprint.iacr.org/2020/1261)

#### Videos

* [MuSig2 in Taproot - Nicolas Dorier - higher level explanation](https://www.youtube.com/watch?v=hrUyGW91JBc)
* [MuSig2: Simple Two-Round Schnorr Multi-Signatures - very detailed, more crypto focused explanation](https://www.youtube.com/watch?v=Dzqj236cVHk)

### ROAST
[Roast scheme explanation](https://www.youtube.com/watch?v=f2soc95MWWY)

### <u>Sighash Types</u>

Maybe sighash flags could be a useful tool?
It's possible to construct a transaction with multiple inputs/outputs from different parties using signatures committing only to specific inputs or outputs.
#### Canvas

![[Signature and Flags.canvas]]
#### Blog articles

[Blog article on Sighashes - good examples - Raghav Sood](https://raghavsood.com/blog/2018/06/10/bitcoin-signature-types-sighash)

[Signature Hash Flags Medium Article - enigbe ochekliye](https://enigbe.medium.com/signature-hash-flags-f059d035ddd0)



### <u>How to make taproot transactions? (Example transactions)</u>

  

https://github.com/danielabrozzoni/multisigs_and_carrots (Best Example I could find)

https://bitcoindevkit.org/blog/2021/11/first-bdk-taproot-tx-look-at-the-code-part-1/ (Interesting read)

https://github.com/bitcoin-core/btcdeb/blob/master/doc/tapscript-example-with-tap.md (Good code for spending taproot transactions)

https://dev.to/eunovo/a-guide-to-creating-taproot-scripts-with-bitcoinjs-lib-4oph (Can look at this, this uses bitcoinjs-lib to create taproot scripts)


### <u>Partially signed bitcoin transactions</u>

[Bitcoin Optech collection of sources](https://bitcoinops.org/en/topics/psbt/)


  
