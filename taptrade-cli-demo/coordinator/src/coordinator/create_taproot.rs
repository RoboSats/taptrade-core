use bitcoin::Address;
use bdk::miniscript::psbt::PsbtExt;
use bitcoin::Network;
use bitcoin::taproot::TaprootSpendInfo;
use miniscript::{Miniscript, Descriptor, DescriptorPublicKey};
use bdk::bitcoin::psbt::PartiallySignedTransaction;
use bdk::blockchain::EsploraBlockchain;
use std::str::FromStr;
use bdk::bitcoin::secp256k1::Secp256k1;

fn combine_and_broadcast() -> Result<(), Box<dyn std::error::Error>> {
    let mut base_psbt = PartiallySignedTransaction::from_str("TODO: insert the psbt created in step 3 here")?;
    let signed_psbts = vec![
         // TODO: Paste each participant's PSBT here
         "makers_psbt",
         "takers_psbt",
    ];

    for psbt in signed_psbts {
        let psbt = PartiallySignedTransaction::from_str(psbt)?;
        base_psbt.combine(psbt)?;
    }

    let secp = Secp256k1::new();
    let psbt = base_psbt.finalize(&secp).unwrap();
    let finalized_tx = psbt.extract_tx();
    dbg!(finalized_tx.txid());

    let blockchain = EsploraBlockchain::new("https://blockstream.info/testnet/api", 20);
    dbg!(blockchain.broadcast(&finalized_tx));
    Ok(())
}

// fn create_taproot_psbt(inputs: Vec<UTXO>, outputs: Vec<Output>) -> PartiallySignedTransaction {
//     let secp = Secp256k1::new();
//     let mut psbt = PartiallySignedTransaction::new();

//     // Add inputs
//     for input in inputs {
//         psbt.inputs.push(input.to_psbt_input());
//     }

//     // Add outputs
//     for output in outputs {
//         psbt.outputs.push(output.to_psbt_output());
//     }

//     // Add Taproot data
//     // call create_script here, and add descriptor here
//     let taproot_info = TaprootSpendInfo::new(secp, root);
//     psbt.global.taproot_spend_info = Some(taproot_info);

//     psbt
// }
// struct UTXO {

// }
// impl UTXO {
//     fn to_psbt_input(&self) -> PsbtInput {
//         PsbtInput {
//             witness_utxo: Some(self.clone()),
//             ..Default::default()
//         }
//     }
// }
// struct Output{

// }
// impl Output {
//     fn to_psbt_output(&self) -> PsbtOutput {
//         PsbtOutput {
//             value: self.amount,
//             script: self.taproot_script.clone(),
//             ..Default::default()
//         }
//     }
// }

// fn sign_psbt(psbt: &mut PartiallySignedTransaction, privkey: SecretKey) {
//     let secp = Secp256k1::new();

//     for (index, input) in psbt.inputs.iter_mut().enumerate() {
//         let sighash = psbt.sighash(index, secp);
//         let signature = SchnorrSig::sign(sighash, &privkey, secp);
//         input.taproot_key_sig = Some(signature);
//     }
// }


// async fn create_script(coordinator_pub_key, maker_pub_key,taker_pub_key ) {
//     // Define the Miniscript policies
//     let policy_a = format!("and_v(v:pk({}),and_v(v:pk({}),after(144)))", coordinator_pub_key, maker_pub_key);
//     let policy_b = format!("and_v(v:pk({}),and_v(v:pk({}),pk({})))", maker_pub_key, taker_pub_key, coordinator_pub_key);
//     let policy_c = format!("and_v(v:pk({}),pk({}))", maker_pub_key, coordinator_pub_key);
//     let policy_d = format!("and_v(v:pk({}),pk({}))", taker_pub_key, coordinator_pub_key);
//     let policy_e = format!("and_v(v:pk({}),after(12228))", maker_pub_key);
//     let policy_f = format!("and_v(and_v(v:pk({}),v:pk({})),after(2048))", maker_pub_key, taker_pub_key);

//     // Compile the policies into Miniscript
//     let miniscript_a: Miniscript<DescriptorPublicKey> = policy_a.parse().unwrap().compile().unwrap();
//     let miniscript_b: Miniscript<DescriptorPublicKey> = policy_b.parse().unwrap().compile().unwrap();
//     let miniscript_c: Miniscript<DescriptorPublicKey> = policy_c.parse().unwrap().compile().unwrap();
//     let miniscript_d: Miniscript<DescriptorPublicKey> = policy_d.parse().unwrap().compile().unwrap();
//     let miniscript_e: Miniscript<DescriptorPublicKey> = policy_e.parse().unwrap().compile().unwrap();
//     let miniscript_f: Miniscript<DescriptorPublicKey> = policy_f.parse().unwrap().compile().unwrap();

//     // Create the Taproot descriptors
//     let descriptor_a = Descriptor::Tr(coordinator_pub_key.clone(), vec![miniscript_a]);
//     let descriptor_b = Descriptor::Tr(coordinator_pub_key.clone(), vec![miniscript_b]);
//     let descriptor_c = Descriptor::Tr(coordinator_pub_key.clone(), vec![miniscript_c]);
//     let descriptor_d = Descriptor::Tr(coordinator_pub_key.clone(), vec![miniscript_d]);
//     let descriptor_e = Descriptor::Tr(coordinator_pub_key.clone(), vec![miniscript_e]);
//     let descriptor_f = Descriptor::Tr(coordinator_pub_key.clone(), vec![miniscript_f]);

//     // Generate the Taproot addresses
//     let address_a = Address::p2tr(&descriptor_a, Network::Bitcoin);
//     let address_b = Address::p2tr(&descriptor_b, Network::Bitcoin);
//     let address_c = Address::p2tr(&descriptor_c, Network::Bitcoin);
//     let address_d = Address::p2tr(&descriptor_d, Network::Bitcoin);
//     let address_e = Address::p2tr(&descriptor_e, Network::Bitcoin);
//     let address_f = Address::p2tr(&descriptor_f, Network::Bitcoin);

//     println!("Taproot Address A: {}", address_a);
//     println!("Taproot Address B: {}", address_b);
//     println!("Taproot Address C: {}", address_c);
//     println!("Taproot Address D: {}", address_d);
//     println!("Taproot Address E: {}", address_e);
//     println!("Taproot Address F: {}", address_f);

// }

