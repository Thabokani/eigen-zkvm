#![no_std]
use revm::{
    db::CacheState,
    interpreter::CreateScheme,
    primitives::{
        Address,
        calc_excess_blob_gas, keccak256, Env, SpecId, AccountInfo, Bytecode, TransactTo, U256,
    },
};
//use runtime::{print, get_prover_input, coprocessors::{get_data, get_data_len}};
use powdr_riscv_rt::{print, coprocessors::{get_data, get_data_len}};

use models::*;

extern crate alloc;
use alloc::vec::Vec;
use alloc::vec;
use alloc::string::String;
use alloc::string::ToString;

#[no_mangle]
fn main() {
    let suite_len = get_data_len(666);
    let mut suite_json = vec![0; suite_len];
    get_data(666, &mut suite_json);
    let suite_json: Vec<u8> = suite_json.into_iter().map(|x| x as u8).collect();
    let suite_json_str = String::from_utf8(suite_json).unwrap();
    let suite = read_suite(&suite_json_str);

    let chain_id_len = get_data_len(667);
    let mut chain_id_in = vec![0; chain_id_len];
    get_data(667, &mut chain_id_in);
    let chain_id: u64 = chain_id_in[0].into();
    print!("chain_id: {chain_id}\n");

    let addr_len = get_data_len(668);
    let mut addr_in = vec![0; addr_len];
    print!("addr: {addr_len}\n");
    get_data(668, &mut addr_in);
    let addr_in: Vec<u8> = addr_in.into_iter().map(|x| x as u8).collect();
    let addr_in = String::from_utf8(addr_in).unwrap();
    print!("addr: {:?}\n", addr_in);
    let addr: Address = addr_in.parse().unwrap();

    /*
    let chain_id = 1;
    let addr = address!("a94f5374fce5edbc8e2a8697c15331677e6ebf0b");
    */

    assert!(execute_test(&suite, addr, chain_id).is_ok());
}

fn read_suite(s: &String) -> TestUnit {
    let suite: TestUnit = serde_json::from_str(s).map_err(|e| e).unwrap();
    suite
}

fn execute_test(unit: &TestUnit, addr: Address, chain_id: u64) -> Result<(), String> {
    // Create database and insert cache
    let mut cache_state = CacheState::new(false);
    for (address, info) in &unit.pre {
        let acc_info = AccountInfo {
            balance: info.balance,
            code_hash: keccak256(&info.code),
            code: Some(Bytecode::new_raw(info.code.clone())),
            nonce: info.nonce,
        };
        cache_state.insert_account_with_storage(*address, acc_info, info.storage.clone());
    }

    let mut env = Env::default();
    // for mainnet
    env.cfg.chain_id = chain_id;
    // env.cfg.spec_id is set down the road

    // block env
    env.block.number = unit.env.current_number;
    env.block.coinbase = unit.env.current_coinbase;
    env.block.timestamp = unit.env.current_timestamp;
    env.block.gas_limit = unit.env.current_gas_limit;
    env.block.basefee = unit.env.current_base_fee.unwrap_or_default();
    env.block.difficulty = unit.env.current_difficulty;
    // after the Merge prevrandao replaces mix_hash field in block and replaced difficulty opcode in EVM.
    env.block.prevrandao = Some(unit.env.current_difficulty.to_be_bytes().into());
    // EIP-4844
    if let (Some(parent_blob_gas_used), Some(parent_excess_blob_gas)) = (
        unit.env.parent_blob_gas_used,
        unit.env.parent_excess_blob_gas,
        ) {
        env.block
            .set_blob_excess_gas_and_price(calc_excess_blob_gas(
                    parent_blob_gas_used.to(),
                    parent_excess_blob_gas.to(),
                    ));
    }

    // tx env
    env.tx.caller = addr; 
    env.tx.gas_price = unit
        .transaction
        .gas_price
        .or(unit.transaction.max_fee_per_gas)
        .unwrap_or_default();
    env.tx.gas_priority_fee = unit.transaction.max_priority_fee_per_gas;
    // EIP-4844
    env.tx.blob_hashes = unit.transaction.blob_versioned_hashes.clone();
    env.tx.max_fee_per_blob_gas = unit.transaction.max_fee_per_blob_gas;

    // post and execution
    for (spec_name, tests) in &unit.post {
        if matches!(
            spec_name,
            SpecName::ByzantiumToConstantinopleAt5
            | SpecName::Constantinople
            | SpecName::Unknown
            ) {
            continue;
        }

        env.cfg.spec_id = spec_name.to_spec_id();

        for test in tests {
            env.tx.gas_limit = unit.transaction.gas_limit[test.indexes.gas].saturating_to();

            env.tx.data = unit
                .transaction
                .data
                .get(test.indexes.data)
                .unwrap()
                .clone();
            env.tx.value = unit.transaction.value[test.indexes.value];

            env.tx.access_list = unit
                .transaction
                .access_lists
                .get(test.indexes.data)
                .and_then(Option::as_deref)
                .unwrap_or_default()
                .iter()
                .map(|item| {
                    (
                        item.address,
                        item.storage_keys
                        .iter()
                        .map(|key| U256::from_be_bytes(key.0))
                        .collect::<Vec<_>>(),
                        )
                })
            .collect();

            let to = match unit.transaction.to {
                Some(add) => TransactTo::Call(add),
                None => TransactTo::Create(CreateScheme::Create),
            };
            env.tx.transact_to = to;

            let mut cache = cache_state.clone();
            cache.set_state_clear_flag(SpecId::enabled(
                    env.cfg.spec_id,
                    revm::primitives::SpecId::SPURIOUS_DRAGON,
                    ));
            let mut state = revm::db::State::builder()
                .with_cached_prestate(cache)
                .with_bundle_update()
                .build();
            let mut evm = revm::new();
            evm.database(&mut state);
            evm.env = env.clone();

            // do the deed
            let exec_result = evm.transact_commit();

            // validate results
            // this is in a closure so we can have a common printing routine for errors
            let check = || {
                // if we expect exception revm should return error from execution.
                // So we do not check logs and state root.
                //
                // Note that some tests that have exception and run tests from before state clear
                // would touch the caller account and make it appear in state root calculation.
                // This is not something that we would expect as invalid tx should not touch state.
                // but as this is a cleanup of invalid tx it is not properly defined and in the end
                // it does not matter.
                // Test where this happens: `tests/GeneralStateTests/stTransactionTest/NoSrcAccountCreate.json`
                // and you can check that we have only two "hash" values for before and after state clear.
                match (&test.expect_exception, &exec_result) {
                    // do nothing
                    (None, Ok(_)) => (),
                    // return okay, exception is expected.
                    (Some(_), Err(_e)) => {
                        //print!("ERROR: {e}");
                        return Ok(());
                    }
                    _ => {
                        let s = exec_result.clone().err().map(|e| e.to_string()).unwrap();
                        print!("UNEXPECTED ERROR: {s}");
                        return Err(s);
                    }
                }
                Ok(())
            };

                    // dump state and traces if test failed
                    let Err(e) = check() else { continue };

                    return Err(e);
        }
    }
    Ok(())
}
