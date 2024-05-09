
How to construct a Transaction spending a P2TR input.


### <u>Key Path spending</u>

The Key Path spend is the more ressource efficient and private way to spend the output. 
If the Internal Key was created by a single party, the creator can just generate a Signature. If the internal Key has been generated from multiple parties public keys, for example via MuSig2, these parties have to collaboratively generate a valid Signature.

1. Create Schnorr signature
	* SIGHASH_DEFAULT
2. Put signature in input Witness

### <u>Script Path spending</u>

In case spending condition from the MAST is necessary to spend the output (e.g. Escrow, inactive counterparty) the Transaction input witness has to be constructed of the following elements:

```
witness = [script inputs, script, control block]
```

##### Script inputs
All X script inputs will be the input to the script satisfying it.

##### Script
The spending script contained in the MAST Leaf to be unlocked. Always the penultimate element in the witness.

##### Control Block
Always the last element in the Witness.  Proves inclusion of the script in the MAST.

```
CB = 192 | parity bit | internal (untweaked) key | sibling hashes (deepest first)
```

