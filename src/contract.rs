// SPDX-License-Identifier: MIT
use soroban_sdk::{contract, contractimpl, token::TokenClient, Address, BytesN, Env, Symbol, Vec};

use crate::errors;
use crate::events;
use crate::storage;
use crate::types::{Bounty, BountyMeta, Contributor, Milestone};

const STATUS_OPEN: &str = "open";
const STATUS_IN_PROGRESS: &str = "in_progress";
const STATUS_COMPLETED: &str = "completed";
const STATUS_CANCELLED: &str = "cancelled";
const STATUS_DISPUTED: &str = "disputed";

fn generate_bounty_id(env: &Env, count: u64) -> BytesN<32> {
    let mut buf = [0u8; 32];
    buf[24..32].copy_from_slice(&count.to_be_bytes());
    BytesN::from_array(env, &buf)
}

#[contract]
pub struct MergeMintContract;

#[contractimpl]
impl MergeMintContract {
    pub fn create_bounty(
        env: Env,
        creator: Address,
        title: Symbol,
        description: Symbol,
        reward_amount: i128,
        reward_token: Address,
        min_reputation: u32,
        deadline: Option<u32>,
        milestones: Option<Vec<Milestone>>,
    ) -> BytesN<32> {
        creator.require_auth();

        let count = storage::get_bounty_count(&env);
        let id = generate_bounty_id(&env, count);

        let bounty = Bounty {
            creator: creator.clone(),
            reward_amount,
            reward_token,
            assignees: Vec::new(&env),
            max_assignees: 1,
            status: Symbol::new(&env, STATUS_OPEN),
            min_reputation,
            deadline,
            milestones,
        };

        let meta = BountyMeta { title, description };

        storage::store_bounty(&env, &id, &bounty);
        storage::store_bounty_meta(&env, &id, &meta);
        storage::set_bounty_count(&env, &(count + 1));
        storage::add_bounty_to_status(&env, &id, &bounty.status);

        let mut open = storage::get_open_bounties(&env);
        open.push_back(id.clone());
        storage::set_open_bounties(&env, &open);

        events::emit_bounty_created(&env, &id, &creator, &reward_amount);
        id
    }

    pub fn claim_bounty(env: Env, contributor: Address, bounty_id: BytesN<32>) {
        contributor.require_auth();

        let mut bounty = match storage::get_bounty(&env, &bounty_id) {
            Some(b) => b,
            None => panic!("{}", errors::BOUNTY_NOT_FOUND),
        };

        if bounty.assignees.len() >= bounty.max_assignees {
            panic!("{}", errors::BOUNTY_ALREADY_ASSIGNED);
        }

        for (addr, _) in bounty.assignees.iter() {
            if addr == contributor {
                panic!("{}", errors::BOUNTY_ALREADY_ASSIGNED);
            }
        }

        // #275: use Contributor::new for default construction
        let mut contrib = storage::get_contributor(&env, &contributor)
            .unwrap_or_else(|| Contributor::new(contributor.clone()));

        if contrib.active_claims >= 1 {
            panic!("{}", errors::CONTRIBUTOR_HAS_ACTIVE_CLAIM);
        }

        if let Some(deadline) = bounty.deadline {
            if env.ledger().sequence() > deadline {
                panic!("{}", errors::BOUNTY_DEADLINE_PASSED);
            }
        }

        if bounty.min_reputation > 0 && contrib.reputation < bounty.min_reputation {
            panic!("contributor reputation is too low");
        }

        let previous_status = bounty.status.clone();
        bounty.assignees.push_back((contributor.clone(), 10_000u32));
        bounty.status = Symbol::new(&env, STATUS_IN_PROGRESS);

        storage::store_bounty(&env, &bounty_id, &bounty);
        storage::move_bounty_status(&env, &bounty_id, &previous_status, &bounty.status);

        contrib.active_claims += 1;
        storage::store_contributor(&env, &contributor, &contrib);

        events::emit_bounty_claimed(&env, &bounty_id, &contributor);

        let open = storage::get_open_bounties(&env);
        let mut new_open = Vec::new(&env);
        for existing_id in open.iter() {
            if existing_id != bounty_id {
                new_open.push_back(existing_id);
            }
        }
        storage::set_open_bounties(&env, &new_open);
    }

    pub fn complete_bounty(env: Env, verifier: Address, bounty_id: BytesN<32>) {
        verifier.require_auth();

        let mut bounty = match storage::get_bounty(&env, &bounty_id) {
            Some(b) => b,
            None => panic!("{}", errors::BOUNTY_NOT_FOUND),
        };

        if bounty.assignees.is_empty() {
            panic!("{}", errors::BOUNTY_HAS_NO_ASSIGNEE);
        }

        let token = TokenClient::new(&env, &bounty.reward_token);

        // #270: complete any remaining milestones
        if let Some(ref mut milestones) = bounty.milestones {
            let (primary_assignee, _) = bounty.assignees.get(0).unwrap();
            let mut updated = Vec::new(&env);
            for (i, mut ms) in milestones.iter().enumerate() {
                if !ms.completed {
                    token.transfer(&verifier, &primary_assignee, &ms.reward);
                    Self::credit_contributor(&env, &primary_assignee, ms.reward);
                    storage::append_contributor_bounty(&env, &primary_assignee, &bounty_id);
                    events::emit_milestone_completed(&env, &bounty_id, &primary_assignee, i as u32);
                    ms.completed = true;
                }
                updated.push_back(ms);
            }
            bounty.milestones = Some(updated);
        } else {
            // No milestones — distribute reward by basis-point shares
            for (assignee, share_bp) in bounty.assignees.iter() {
                let payout =
                    (bounty.reward_amount as i128) * (share_bp as i128) / 10_000_i128;
                token.transfer(&verifier, &assignee, &payout);
                Self::credit_contributor(&env, &assignee, payout);
                // #269: append to contributor bounty index
                storage::append_contributor_bounty(&env, &assignee, &bounty_id);
                events::emit_reward_paid(&env, &bounty_id, &assignee, &payout);
            }
        }

        let (primary_assignee, _) = bounty.assignees.get(0).unwrap();
        let previous_status = bounty.status.clone();
        bounty.status = Symbol::new(&env, STATUS_COMPLETED);
        storage::store_bounty(&env, &bounty_id, &bounty);
        storage::move_bounty_status(&env, &bounty_id, &previous_status, &bounty.status);

        events::emit_bounty_completed(&env, &bounty_id, &primary_assignee);
    }

    /// #270: complete a single milestone by index, releasing its partial reward.
    pub fn complete_milestone(
        env: Env,
        verifier: Address,
        bounty_id: BytesN<32>,
        milestone_index: u32,
    ) {
        verifier.require_auth();

        let mut bounty = match storage::get_bounty(&env, &bounty_id) {
            Some(b) => b,
            None => panic!("{}", errors::BOUNTY_NOT_FOUND),
        };

        if bounty.assignees.is_empty() {
            panic!("{}", errors::BOUNTY_HAS_NO_ASSIGNEE);
        }

        let milestones = match bounty.milestones {
            Some(ref ms) => ms.clone(),
            None => panic!("bounty has no milestones"),
        };

        let idx = milestone_index as usize;
        if idx >= milestones.len() as usize {
            panic!("milestone index out of range");
        }

        let mut ms = milestones.get(milestone_index).unwrap();
        if ms.completed {
            panic!("milestone already completed");
        }

        let (assignee, _) = bounty.assignees.get(0).unwrap();
        let token = TokenClient::new(&env, &bounty.reward_token);
        token.transfer(&verifier, &assignee, &ms.reward);

        Self::credit_contributor(&env, &assignee, ms.reward);
        storage::append_contributor_bounty(&env, &assignee, &bounty_id);

        ms.completed = true;
        let mut updated = Vec::new(&env);
        for (i, m) in milestones.iter().enumerate() {
            if i as u32 == milestone_index {
                updated.push_back(ms.clone());
            } else {
                updated.push_back(m);
            }
        }
        bounty.milestones = Some(updated);
        storage::store_bounty(&env, &bounty_id, &bounty);

        events::emit_milestone_completed(&env, &bounty_id, &assignee, milestone_index);
    }

    pub fn raise_dispute(env: Env, caller: Address, bounty_id: BytesN<32>) {
        caller.require_auth();

        let mut bounty = storage::get_bounty(&env, &bounty_id).expect("bounty not found");

        let is_assignee = bounty.assignees.iter().any(|(addr, _)| addr == caller);
        if caller != bounty.creator && !is_assignee {
            panic!("only creator or assignee can raise dispute");
        }

        let previous_status = bounty.status.clone();
        bounty.status = Symbol::new(&env, STATUS_DISPUTED);
        storage::store_bounty(&env, &bounty_id, &bounty);
        storage::move_bounty_status(&env, &bounty_id, &previous_status, &bounty.status);
        events::emit_bounty_disputed(&env, &bounty_id, &caller);
    }

    pub fn update_contributor_metadata(env: Env, contributor: Address, metadata: Symbol) {
        contributor.require_auth();

        // #275: use Contributor::new for default construction
        let mut contrib = storage::get_contributor(&env, &contributor)
            .unwrap_or_else(|| Contributor::new(contributor.clone()));

        contrib.metadata = Some(metadata);
        storage::store_contributor(&env, &contributor, &contrib);
    }

    pub fn cancel_bounty(env: Env, caller: Address, bounty_id: BytesN<32>) {
        caller.require_auth();

        let mut bounty = storage::get_bounty(&env, &bounty_id).expect("bounty not found");

        if caller != bounty.creator {
            panic!("{}", errors::NOT_BOUNTY_CREATOR);
        }

        if bounty.status != Symbol::new(&env, STATUS_OPEN) {
            panic!("{}", errors::BOUNTY_NOT_OPEN);
        }

        bounty.status = Symbol::new(&env, STATUS_CANCELLED);
        storage::store_bounty(&env, &bounty_id, &bounty);
        events::emit_bounty_cancelled(&env, &bounty_id, &caller);
    }

    pub fn expire_bounty(env: Env, caller: Address, bounty_id: BytesN<32>) {
        caller.require_auth();

        let mut bounty = storage::get_bounty(&env, &bounty_id).expect("bounty not found");

        let deadline = match bounty.deadline {
            Some(d) => d,
            None => panic!("{}", errors::BOUNTY_NO_DEADLINE),
        };

        if env.ledger().sequence() <= deadline {
            panic!("{}", errors::DEADLINE_NOT_PASSED);
        }

        if bounty.status != Symbol::new(&env, STATUS_OPEN) {
            panic!("{}", errors::BOUNTY_NOT_OPEN);
        }

        bounty.status = Symbol::new(&env, STATUS_CANCELLED);
        storage::store_bounty(&env, &bounty_id, &bounty);
        events::emit_bounty_expired(&env, &bounty_id, &bounty.creator);
    }

    // --- Read-only ---

    pub fn get_bounty(env: Env, bounty_id: BytesN<32>) -> Option<Bounty> {
        storage::get_bounty(&env, &bounty_id)
    }

    pub fn get_bounty_meta(env: Env, bounty_id: BytesN<32>) -> Option<BountyMeta> {
        storage::get_bounty_meta(&env, &bounty_id)
    }

    pub fn get_contributor(env: Env, address: Address) -> Option<Contributor> {
        storage::get_contributor(&env, &address)
    }

    pub fn get_bounty_count(env: Env) -> u64 {
        storage::get_bounty_count(&env)
    }

    pub fn get_bounties_by_status(env: Env, status: Symbol) -> Vec<BytesN<32>> {
        storage::get_bounties_by_status(&env, &status)
    }

    pub fn get_open_bounties(env: Env) -> Vec<BytesN<32>> {
        storage::get_open_bounties(&env)
    }

    /// #269: return the list of bounty IDs a contributor has completed.
    pub fn get_bounties_by_contributor(env: Env, contributor: Address) -> Vec<BytesN<32>> {
        storage::get_contributor_bounties(&env, &contributor)
    }

    /// #266: return up to `limit` bounties starting at counter index `start`.
    pub fn get_bounties_by_range(env: Env, start: u64, limit: u64) -> Vec<Bounty> {
        let count = storage::get_bounty_count(&env);
        let mut result = Vec::new(&env);
        if start >= count {
            return result;
        }
        let end = (start + limit).min(count);
        for i in start..end {
            let id = generate_bounty_id(&env, i);
            if let Some(bounty) = storage::get_bounty(&env, &id) {
                result.push_back(bounty);
            }
        }
        result
    }

    // --- Private helpers ---

    /// #275: update contributor stats after a payout (used by complete_bounty and complete_milestone).
    fn credit_contributor(env: &Env, assignee: &Address, payout: i128) {
        let mut contrib = storage::get_contributor(env, assignee)
            .unwrap_or_else(|| Contributor::new(assignee.clone()));
        contrib.reputation += 10;
        contrib.total_earned += payout;
        contrib.contribution_count += 1;
        if contrib.active_claims > 0 {
            contrib.active_claims -= 1;
        }
        storage::store_contributor(env, assignee, &contrib);
    }
}
