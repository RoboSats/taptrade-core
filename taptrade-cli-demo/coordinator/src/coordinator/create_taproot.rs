use bitcoin::blockdata::transaction::{Transaction, TxIn, TxOut};
use bitcoin::blockdata::script::Builder;
use bitcoin::consensus::encode;
use miniscript::{Miniscript, Descriptor, DescriptorPublicKey, policy::Concrete};
use miniscript::bitcoin::secp256k1::{Secp256k1, SecretKey, PublicKey, Message, Signature};
// use miniscript::bitcoin::util::bip32::{ExtendedPrivKey, ExtendedPubKey};
use std::str::FromStr;
use bitcoin::psbt::PartiallySignedTransaction;
use bitcoin::util::taproot::{TaprootBuilder, TaprootSpendInfo};
use bitcoin::util::schnorr::SchnorrSig;
use bitcoin::secp256k1::{Secp256k1, SecretKey};
use bitcoin::Transaction;



fn create_taproot_psbt(inputs: Vec<UTXO>, outputs: Vec<Output>) -> PartiallySignedTransaction {
    let secp = Secp256k1::new();
    let mut psbt = PartiallySignedTransaction::new();

    // Add inputs
    for input in inputs {
        psbt.inputs.push(input.to_psbt_input());
    }

    // Add outputs
    for output in outputs {
        psbt.outputs.push(output.to_psbt_output());
    }

    // Add Taproot data
    // call create_script here, and add descriptor here
    let taproot_info = TaprootSpendInfo::new(secp, root);
    psbt.global.taproot_spend_info = Some(taproot_info);

    psbt
}

impl UTXO {
    fn to_psbt_input(&self) -> PsbtInput {
        PsbtInput {
            witness_utxo: Some(self.clone()),
            ..Default::default()
        }
    }
}
impl Output {
    fn to_psbt_output(&self) -> PsbtOutput {
        PsbtOutput {
            value: self.amount,
            script: self.taproot_script.clone(),
            ..Default::default()
        }
    }
}

fn sign_psbt(psbt: &mut PartiallySignedTransaction, privkey: SecretKey) {
    let secp = Secp256k1::new();

    for (index, input) in psbt.inputs.iter_mut().enumerate() {
        let sighash = psbt.sighash(index, secp);
        let signature = SchnorrSig::sign(sighash, &privkey, secp);
        input.taproot_key_sig = Some(signature);
    }
}


async fn create_script(coordinator_pub_key, maker_pub_key,taker_pub_key ) {
    // Define the Miniscript policies
    let policy_a = format!("and_v(v:pk({}),and_v(v:pk({}),after(144)))", coordinator_pub_key, maker_pub_key);
    let policy_b = format!("and_v(v:pk({}),and_v(v:pk({}),pk({})))", maker_pub_key, taker_pub_key, coordinator_pub_key);
    let policy_c = format!("and_v(v:pk({}),pk({}))", maker_pub_key, coordinator_pub_key);
    let policy_d = format!("and_v(v:pk({}),pk({}))", taker_pub_key, coordinator_pub_key);
    let policy_e = format!("and_v(v:pk({}),after(12228))", maker_pub_key);
    let policy_f = format!("and_v(and_v(v:pk({}),v:pk({})),after(2048))", maker_pub_key, taker_pub_key);

    // Compile the policies into Miniscript
    let miniscript_a: Miniscript<DescriptorPublicKey> = policy_a.parse().unwrap().compile().unwrap();
    let miniscript_b: Miniscript<DescriptorPublicKey> = policy_b.parse().unwrap().compile().unwrap();
    let miniscript_c: Miniscript<DescriptorPublicKey> = policy_c.parse().unwrap().compile().unwrap();
    let miniscript_d: Miniscript<DescriptorPublicKey> = policy_d.parse().unwrap().compile().unwrap();
    let miniscript_e: Miniscript<DescriptorPublicKey> = policy_e.parse().unwrap().compile().unwrap();
    let miniscript_f: Miniscript<DescriptorPublicKey> = policy_f.parse().unwrap().compile().unwrap();

    // Create the Taproot descriptors
    let descriptor_a = Descriptor::Tr(coordinator_pub_key.clone(), vec![miniscript_a]);
    let descriptor_b = Descriptor::Tr(coordinator_pub_key.clone(), vec![miniscript_b]);
    let descriptor_c = Descriptor::Tr(coordinator_pub_key.clone(), vec![miniscript_c]);
    let descriptor_d = Descriptor::Tr(coordinator_pub_key.clone(), vec![miniscript_d]);
    let descriptor_e = Descriptor::Tr(coordinator_pub_key.clone(), vec![miniscript_e]);
    let descriptor_f = Descriptor::Tr(coordinator_pub_key.clone(), vec![miniscript_f]);

    // Generate the Taproot addresses
    let address_a = Address::p2tr(&descriptor_a, Network::Bitcoin);
    let address_b = Address::p2tr(&descriptor_b, Network::Bitcoin);
    let address_c = Address::p2tr(&descriptor_c, Network::Bitcoin);
    let address_d = Address::p2tr(&descriptor_d, Network::Bitcoin);
    let address_e = Address::p2tr(&descriptor_e, Network::Bitcoin);
    let address_f = Address::p2tr(&descriptor_f, Network::Bitcoin);

    println!("Taproot Address A: {}", address_a);
    println!("Taproot Address B: {}", address_b);
    println!("Taproot Address C: {}", address_c);
    println!("Taproot Address D: {}", address_d);
    println!("Taproot Address E: {}", address_e);
    println!("Taproot Address F: {}", address_f);

}


async fn procedure() {
    let inputs = vec![/* ... UTXOs ... */ ];
    let outputs = vec![/* ... Outputs ... */];

    let mut psbt = create_taproot_psbt(inputs, outputs);
    let privkey = SecretKey::from_slice(&[/* private key bytes */]).unwrap();

    sign_psbt(&mut psbt, privkey);

    // Finalize and broadcast the PSBT
    let tx = psbt.finalize().unwrap();
    broadcast_transaction(tx);
}