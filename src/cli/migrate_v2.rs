use crate::core::crypto::{
    delete_master_key_v1, read_master_key_v1, read_master_key_v2, write_master_key_v2,
};
use crate::core::master_password::{prompt_password_with_confirmation, wrap_master_key};
use anyhow::Result;

pub fn migrate_v2() -> Result<()> {
    if read_master_key_v2()?.is_some() {
        println!("v4 master-key-v2 already present — nothing to migrate.");
        return Ok(());
    }
    let legacy = match read_master_key_v1()? {
        Some(k) => k,
        None => {
            println!("No v3 master-key-v1 found. Set a fresh master password instead.");
            return set_fresh_password();
        }
    };
    println!("Found v3 master-key-v1. Setting up v4 master-password wrap.");
    let pw = prompt_password_with_confirmation()?;
    let wrapped = wrap_master_key(&legacy, &pw)?;
    write_master_key_v2(&wrapped)?;
    delete_master_key_v1()?;
    println!("Migrated. Master key is now password-wrapped under master-key-v2.");
    Ok(())
}

fn set_fresh_password() -> Result<()> {
    let pw = prompt_password_with_confirmation()?;
    let mut master = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut master);
    let wrapped = wrap_master_key(&master, &pw)?;
    write_master_key_v2(&wrapped)?;
    println!("New master key set under master-key-v2.");
    Ok(())
}
