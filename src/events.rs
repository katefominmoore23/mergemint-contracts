// SPDX-License-Identifier: MIT
use soroban_sdk::{Address, BytesN, Env, Symbol};

pub fn emit_bounty_created(env: &Env, bounty_id: &BytesN<32>, creator: &Address, reward: &i128) {
    env.events().publish(
        (Symbol::new(env, "bounty_created"), creator.clone()),
        (bounty_id.clone(), *reward),
    );
}

pub fn emit_bounty_claimed(env: &Env, bounty_id: &BytesN<32>, contributor: &Address) {
    env.events().publish(
        (Symbol::new(env, "bounty_claimed"), contributor.clone()),
        bounty_id.clone(),
    );
}

pub fn emit_bounty_completed(env: &Env, bounty_id: &BytesN<32>, contributor: &Address) {
    env.events().publish(
        (Symbol::new(env, "bounty_completed"), contributor.clone()),
        bounty_id.clone(),
    );
}

pub fn emit_reward_paid(env: &Env, bounty_id: &BytesN<32>, contributor: &Address, amount: &i128) {
    env.events().publish(
        (Symbol::new(env, "reward_paid"), contributor.clone()),
        (bounty_id.clone(), *amount),
    );
}

pub fn emit_milestone_completed(
    env: &Env,
    bounty_id: &BytesN<32>,
    contributor: &Address,
    index: u32,
) {
    env.events().publish(
        (Symbol::new(env, "milestone_completed"), contributor.clone()),
        (bounty_id.clone(), index),
    );
}

pub fn emit_bounty_disputed(env: &Env, bounty_id: &BytesN<32>, caller: &Address) {
    env.events().publish(
        (Symbol::new(env, "bounty_disputed"), caller.clone()),
        bounty_id.clone(),
    );
}

pub fn emit_bounty_cancelled(env: &Env, bounty_id: &BytesN<32>, creator: &Address) {
    env.events().publish(
        (Symbol::new(env, "bounty_cancelled"), creator.clone()),
        bounty_id.clone(),
    );
}

pub fn emit_bounty_expired(env: &Env, bounty_id: &BytesN<32>, creator: &Address) {
    env.events().publish(
        (Symbol::new(env, "bounty_expired"), creator.clone()),
        bounty_id.clone(),
    );
}
