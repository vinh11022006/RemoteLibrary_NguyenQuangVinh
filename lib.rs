#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Bytes, Env,
    Val, Vec, IntoVal,
};

// ===========================
// Storage keys
// ===========================

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    NextId,
    DefaultPayToken,
    Owner(u128),
    Creator(u128),
    RoyaltyBps(u128),
    Uri(u128),
    FanPoints(Address),
}

// ===========================
// Errors
// ===========================

#[contracterror]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Error {
    NotAuthorized = 1,
    TokenNotFound = 2,
    InvalidRoyalty = 3,
    InvalidPrice = 4,
    InvalidPaymentToken = 5,
    Overflow = 6,
    NotOwner = 20,
    SameOwner = 21,
    PaymentFailed = 22,
}

// ===========================
// Types
// ===========================

#[contracttype]
#[derive(Clone)]
pub struct TokenId(pub u128);

#[contracttype]
#[derive(Clone)]
pub struct NftInfo {
    pub token_id: TokenId,
    pub owner: Address,
    pub creator: Address,
    pub royalty_bps: u32,
    pub uri: Bytes,
}

// ===========================
// Main contract
// ===========================

#[contract]
pub struct FanRewardsNftMarket;

#[contractimpl]
impl FanRewardsNftMarket {
    pub fn set_default_payment_token(env: Env, admin: Address, token: Address) {
        admin.require_auth();
        env.storage()
            .instance()
            .set::<DataKey, Address>(&DataKey::DefaultPayToken, &token);
    }

    pub fn get_default_payment_token(env: Env) -> Option<Address> {
        env.storage()
            .instance()
            .get::<DataKey, Address>(&DataKey::DefaultPayToken)
    }

    pub fn mint(
        env: Env,
        creator: Address,
        initial_owner: Address,
        royalty_bps: u32,
        uri: Bytes,
    ) -> Result<TokenId, Error> {
        creator.require_auth();
        if royalty_bps > 10_000 {
            return Err(Error::InvalidRoyalty);
        }

        let id = next_id(&env)?;
        let tid = TokenId(id);

        set_owner(&env, id, &initial_owner);
        set_creator(&env, id, &creator);
        set_royalty_bps(&env, id, royalty_bps);
        set_uri(&env, id, &uri);

        Ok(tid)
    }

    pub fn get_info(env: Env, token_id: TokenId) -> Result<NftInfo, Error> {
        let id = token_id.0;
        let owner = get_owner(&env, id).ok_or(Error::TokenNotFound)?;
        let creator = get_creator(&env, id).ok_or(Error::TokenNotFound)?;
        let royalty_bps = get_royalty_bps(&env, id).ok_or(Error::TokenNotFound)?;
        let uri = get_uri(&env, id).ok_or(Error::TokenNotFound)?;

        Ok(NftInfo {
            token_id,
            owner,
            creator,
            royalty_bps,
            uri,
        })
    }

    pub fn transfer(env: Env, token_id: TokenId, from: Address, to: Address) -> Result<(), Error> {
        let id = token_id.0;
        let owner = get_owner(&env, id).ok_or(Error::TokenNotFound)?;
        if owner != from {
            return Err(Error::NotOwner);
        }
        from.require_auth();
        if from == to {
            return Err(Error::SameOwner);
        }
        set_owner(&env, id, &to);
        Ok(())
    }

    pub fn get_fan_points(env: Env, fan: Address) -> u128 {
        env.storage()
            .instance()
            .get::<DataKey, u128>(&DataKey::FanPoints(fan))
            .unwrap_or(0u128)
    }

    pub fn award_fan_points(
        env: Env,
        granter: Address,
        fan: Address,
        points: u128,
    ) -> Result<(), Error> {
        granter.require_auth();
        let current: u128 = env
            .storage()
            .instance()
            .get::<DataKey, u128>(&DataKey::FanPoints(fan.clone()))
            .unwrap_or(0u128);
        let new_total: u128 = current.checked_add(points).ok_or(Error::Overflow)?;
        env.storage()
            .instance()
            .set::<DataKey, u128>(&DataKey::FanPoints(fan), &new_total);
        Ok(())
    }

    pub fn buy(
        env: Env,
        token_id: TokenId,
        buyer: Address,
        price: i128,
        payment_token: Option<Address>,
    ) -> Result<(), Error> {
        buyer.require_auth();
        if price <= 0 {
            return Err(Error::InvalidPrice);
        }

        let id = token_id.0;
        let owner = get_owner(&env, id).ok_or(Error::TokenNotFound)?;
        if owner == buyer {
            return Err(Error::SameOwner);
        }
        let creator = get_creator(&env, id).ok_or(Error::TokenNotFound)?;
        let royalty_bps = get_royalty_bps(&env, id).ok_or(Error::TokenNotFound)?;

        let pay_token = match payment_token {
            Some(addr) => addr,
            None => env
                .storage()
                .instance()
                .get::<DataKey, Address>(&DataKey::DefaultPayToken)
                .ok_or(Error::InvalidPaymentToken)?,
        };

        let royalty = safe_mul_div(price, royalty_bps as i128, 10_000).ok_or(Error::Overflow)?;
        let seller_amount = price.checked_sub(royalty).ok_or(Error::Overflow)?;

        token_transfer_from(&env, &pay_token, &buyer, &creator, royalty)?;
        token_transfer_from(&env, &pay_token, &buyer, &owner, seller_amount)?;

        set_owner(&env, id, &buyer);

        let points: u128 = if price > 0 { price as u128 } else { 0u128 };
        add_fan_points(&env, &buyer, points)?;

        Ok(())
    }
}

// ===========================
// Internal helpers
// ===========================

fn next_id(env: &Env) -> Result<u128, Error> {
    let current: u128 = env
        .storage()
        .instance()
        .get::<DataKey, u128>(&DataKey::NextId)
        .unwrap_or(0u128);
    let next: u128 = current.checked_add(1u128).ok_or(Error::Overflow)?;
    env.storage()
        .instance()
        .set::<DataKey, u128>(&DataKey::NextId, &next);
    Ok(next)
}

fn set_owner(env: &Env, id: u128, owner: &Address) {
    env.storage()
        .instance()
        .set::<DataKey, Address>(&DataKey::Owner(id), owner);
}
fn get_owner(env: &Env, id: u128) -> Option<Address> {
    env.storage().instance().get::<DataKey, Address>(&DataKey::Owner(id))
}
fn set_creator(env: &Env, id: u128, creator: &Address) {
    env.storage()
        .instance()
        .set::<DataKey, Address>(&DataKey::Creator(id), creator);
}
fn get_creator(env: &Env, id: u128) -> Option<Address> {
    env.storage().instance().get::<DataKey, Address>(&DataKey::Creator(id))
}
fn set_royalty_bps(env: &Env, id: u128, bps: u32) {
    env.storage()
        .instance()
        .set::<DataKey, u32>(&DataKey::RoyaltyBps(id), &bps);
}
fn get_royalty_bps(env: &Env, id: u128) -> Option<u32> {
    env.storage().instance().get::<DataKey, u32>(&DataKey::RoyaltyBps(id))
}
fn set_uri(env: &Env, id: u128, uri: &Bytes) {
    env.storage()
        .instance()
        .set::<DataKey, Bytes>(&DataKey::Uri(id), uri);
}
fn get_uri(env: &Env, id: u128) -> Option<Bytes> {
    env.storage().instance().get::<DataKey, Bytes>(&DataKey::Uri(id))
}
fn fan_key(addr: &Address) -> DataKey {
    DataKey::FanPoints(addr.clone())
}
fn add_fan_points(env: &Env, fan: &Address, points: u128) -> Result<(), Error> {
    if points == 0 {
        return Ok(());
    }
    let current: u128 = env
        .storage()
        .instance()
        .get::<DataKey, u128>(&fan_key(fan))
        .unwrap_or(0u128);
    let new_total: u128 = current.checked_add(points).ok_or(Error::Overflow)?;
    env.storage()
        .instance()
        .set::<DataKey, u128>(&fan_key(fan), &new_total);
    Ok(())
}

// Hàm nhân–chia an toàn
// Hàm nhân–chia an toàn, tránh tràn số
fn safe_mul_div(a: i128, b: i128, c: i128) -> Option<i128> {
    if c == 0 {
        return None;
    }
    let prod = a.checked_mul(b)?;
    prod.checked_div(c)
}

// Hàm gọi cross-contract tới token chuẩn để chuyển tiền
fn token_transfer_from(
    env: &Env,
    token: &Address,
    from: &Address,
    to: &Address,
    amount: i128,
) -> Result<(), Error> {
    if amount <= 0 {
        return Ok(());
    }
    // tên hàm trong token chuẩn Soroban là "xfer_from" (ngắn hơn 9 ký tự)
    let func = symbol_short!("xfer_from");

    let mut args: Vec<Val> = Vec::new(env);
    args.push_back(from.into_val(env));
    args.push_back(to.into_val(env));
    args.push_back(amount.into_val(env));

    // invoke_contract trả về trực tiếp (), nếu lỗi sẽ panic
    env.invoke_contract::<()>(&token, &func, args);
    Ok(())
}