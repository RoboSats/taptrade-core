use bitcoin::Address;
use bdk::descriptor::Descriptor;
use bdk::miniscript::psbt::PsbtExt;
use bitcoin::Network;
use bitcoin::taproot::TaprootSpendInfo;
use bdk::bitcoin::psbt::PartiallySignedTransaction;
use bdk::blockchain::EsploraBlockchain;
use std::str::FromStr;
use bdk::bitcoin::secp256k1::Secp256k1;
use bdk::bitcoin::hashes::hex::FromHex;
use bdk::bitcoin::PublicKey;
use bdk::descriptor;
use bdk::miniscript::descriptor::TapTree;
use bdk::miniscript::policy::Concrete;
// use bdk::miniscript::DummyKey;
use std::sync::Arc;

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


async fn create_script(coordinator_key: &str, maker_key:&str, taker_key:&str ) -> Result<(bdk::descriptor::Descriptor<std::string::String>), Box<dyn std::error::Error>> {

    // let maker_key = "020202020202020202020202020202020202020202020202020202020202020202";
    // let taker_key = "03833be68fb7559c0e62ffdbb6d46cc44a58c19c6ba82e51144b583cff0519c791";
    // let coordinator_key = "03b2f6e8abf3624f8e9b93f7b2567b158c15b0f20ab368f9fcb2d9251d6a788d09";

    // Define policies based on the scripts provided
    let script_a = format!("and(and(after(escrow_timer),pk({})),pk({}))", maker_key, coordinator_key);
    let script_b = format!("and_v(v:pk({}),and_v(v:pk({}),pk({})))", maker_key, taker_key, coordinator_key);
    let script_c = format!("and(pk({}),pk({}))", maker_key, coordinator_key);
    let script_d = format!("and(pk({}),pk({}))", taker_key, coordinator_key);
    let script_e = format!("and(pk({}),after(very_long_timelock))", maker_key);
    let script_f = format!("and_v(and_v(v:pk({}),v:pk({})),after(2048))", maker_key, taker_key);

    // Compile the policies
    let compiled_a = Concrete::<String>::from_str(&script_a)?.compile()?;
    let compiled_b = Concrete::<String>::from_str(&script_b)?.compile()?;
    let compiled_c = Concrete::<String>::from_str(&script_c)?.compile()?;
    let compiled_d = Concrete::<String>::from_str(&script_d)?.compile()?;
    let compiled_e = Concrete::<String>::from_str(&script_e)?.compile()?;
    let compiled_f = Concrete::<String>::from_str(&script_f)?.compile()?;

    // Create TapTree leaves
    let tap_leaf_a = TapTree::Leaf(Arc::new(compiled_a));
    let tap_leaf_b = TapTree::Leaf(Arc::new(compiled_b));
    let tap_leaf_c = TapTree::Leaf(Arc::new(compiled_c));
    let tap_leaf_d = TapTree::Leaf(Arc::new(compiled_d));
    let tap_leaf_e = TapTree::Leaf(Arc::new(compiled_e));
    let tap_leaf_f = TapTree::Leaf(Arc::new(compiled_f));

    // Create the TapTree (example combining leaves, adjust as necessary)
    let tap_tree = TapTree::Tree(Arc::new(tap_leaf_a), Arc::new(tap_leaf_b));

    // Define a dummy internal key (replace with an actual key)
    let dummy_internal_key = "020202020202020202020202020202020202020202020202020202020202020202".to_string();

    // Create the descriptor
    let descriptor = Descriptor::new_tr(dummy_internal_key, Some(tap_tree))?;
    println!("{}", descriptor);

    Ok(descriptor)

}

