// SPDX-License-Identifier: MIT
use soroban_sdk::{contracttype, Address, BytesN, Symbol, Vec};

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum DataKey {
    BountyCount,
    Bounty(BytesN<32>),
    BountyMeta(BytesN<32>),
    Contributor(Address),
    ContributorBounties(Address),
    StatusIndex(Symbol),
    OpenBounties,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct Milestone {
    pub description: Symbol,
    pub reward: i128,
    pub completed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct Bounty {
    pub creator: Address,
    pub reward_amount: i128,
    pub reward_token: Address,
    pub assignees: Vec<(Address, u32)>,
    pub max_assignees: u32,
    #[allow(dead_code)]
    pub status: Symbol,
    pub min_reputation: u32,
    pub deadline: Option<u32>,
    pub milestones: Option<Vec<Milestone>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct BountyMeta {
    pub title: Symbol,
    pub description: Symbol,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct Contributor {
    #[allow(dead_code)]
    pub address: Address,
    pub reputation: u32,
    pub total_earned: i128,
    pub contribution_count: u32,
    pub active_claims: u32,
    pub metadata: Option<Symbol>,
}

impl Contributor {
    pub fn new(address: Address) -> Self {
        Self {
            address,
            reputation: 0,
            total_earned: 0,
            contribution_count: 0,
            active_claims: 0,
            metadata: None,
        }
    }
}
