#![cfg_attr(not(feature = "std"), no_std)]

/// Edit this file to define custom logic or remove it if it is not needed.
/// Learn more about FRAME and the core library of Substrate FRAME pallets:
/// https://substrate.dev/docs/en/knowledgebase/runtime/frame

use codec::{Encode, Decode};

use frame_support::{
    decl_module, decl_storage, decl_event,
    dispatch::DispatchResult, StorageMap, ensure,
    traits::{Currency, ExistenceRequirement, WithdrawReason},
};

use pallet_balances as balances;
use pallet_timestamp as timestamp;
use frame_system::ensure_signed;
use frame_support::traits::Vec;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

const ERR_DID_ALREADY_CLAIMED: &str = "This DID has already been claimed.";
const ERR_DID_NOT_EXIST: &str = "This DID does not exist";
const ERR_DID_NO_OWNER: &str = "No one owens this did";

const ERR_UN_ALREADY_CLAIMED: &str = "This unique name has already been claimed.";

const ERR_LICENSE_INVALID: &str = "Invalid license code";

const ERR_OVERFLOW: &str = "Overflow adding new metadata";
const ERR_UNDERFLOW: &str = "Underflow removing metadata";

const ERR_NOT_OWNER: &str = "You are not the owner";

const ERR_BYTEARRAY_LIMIT_DID: &str = "DID bytearray is too large";
const ERR_BYTEARRAY_LIMIT_NAME: &str = "Name bytearray is too large";

const BYTEARRAY_LIMIT_DID: usize = 100;
const BYTEARRAY_LIMIT_NAME: usize = 50;

const DELETE_LICENSE: u16 = 1;

//TODO: Needs to be updatable via votes!
const FEE_PER_USED_CHAR: u32 = 100;

/// The module's configuration traits are timestamp and balance
pub trait Trait: timestamp::Trait + balances::Trait {
    type Event: From<Event<Self>> + Into<<Self as frame_system::Trait>::Event>;
}


/// Key Metalog struct
#[derive(Encode, Decode, Default, Clone, PartialEq)]
#[cfg_attr(feature = "std", derive(Debug))]
pub struct Metalog<Time> {
    /// DiD
    pub did: Vec<u8>,
    // primary key ,cannot be changed
    /// Unique name
    pub name: Vec<u8>,
    // default=0
    // 0 = no license code
    /// License code
    pub code: u16,
    /// Timestamp
    pub time: Time,
}

decl_storage! {
    trait Store for Module<T: Trait> as Metalog {
        /// Array of personal owned Metalogs data
        OwnedMetaArray get(fn metadata_of_owner_by_index): map hasher(blake2_128_concat) (T::AccountId, u64) => Metalog<T::Moment>;

        /// Number of stored Metalogs per account
        OwnedMetaCount get(fn owner_meta_count): map hasher(blake2_128_concat) T::AccountId => u64;

        /// Index of DID
        OwnedMetaIndex: map hasher(blake2_128_concat) Vec<u8> => u64;

        /// Query for unique names
        UnMeta get(fn meta_of_un): map hasher(blake2_128_concat) Vec<u8> => Metalog<T::Moment>;
        UnOwner get(fn owner_of_un): map hasher(blake2_128_concat) Vec<u8> => Option<T::AccountId>;

        /// Query by DIDs
        DidMeta get(fn meta_of_did): map hasher(blake2_128_concat) Vec<u8> => Metalog<T::Moment>;
        DidOwner get(fn owner_of_did): map hasher(blake2_128_concat) Vec<u8> => Option<T::AccountId>;
    }
}

decl_module! {
    /// The module declaration.
    pub struct Module<T: Trait> for enum Call where origin: T::Origin {
        fn deposit_event() = default;

        // Store initial Metalogs
        #[weight = 0]
        fn create_metalog(
            origin,
            did: Vec<u8>,
            license_code: u16) -> DispatchResult {

            let sender = ensure_signed(origin)?;

            ensure!(did.len() <= BYTEARRAY_LIMIT_DID, ERR_BYTEARRAY_LIMIT_DID);
            ensure!(!<DidOwner<T>>::contains_key(&did), ERR_DID_ALREADY_CLAIMED);
            ensure!(license_code != DELETE_LICENSE, ERR_LICENSE_INVALID);

            let time = <timestamp::Module<T>>::now();

            let mut default_name = Vec::new();
            default_name.push(0);
            let new_metadata = Metalog {
                did,
                name: default_name,
                code: license_code,
                time,
            };

            Self::_owner_store(sender.clone(), new_metadata.clone())?;
            Self::deposit_event(RawEvent::Stored(sender, new_metadata.time, new_metadata.did));
            Ok(())
        }

        // Transfer the ownership, Payment will be implemented in smart contracts
        #[weight = 0]
        fn transfer_ownership(origin, receiver: T::AccountId, did: Vec<u8>) -> DispatchResult {
            let sender = ensure_signed(origin)?;
            Self::_check_did_ownership(sender.clone(), &did)?;
            Self::_transfer(sender.clone(), receiver.clone(), &did)?;

            Self::deposit_event(RawEvent::TransferOwnership(sender, receiver, did));
            Ok(())
        }

        // Buy a unique name
        #[weight = 0]
        pub fn buy_unique_name(origin, did: Vec<u8>, unique_name: Vec<u8>)-> DispatchResult{
            let sender = ensure_signed(origin)?;
            Self::_check_did_ownership(sender.clone(), &did)?;

            ensure!(unique_name.len() <= BYTEARRAY_LIMIT_NAME, ERR_BYTEARRAY_LIMIT_NAME);

            ensure!(!<UnOwner<T>>::contains_key(&unique_name), ERR_UN_ALREADY_CLAIMED);

            let length = unique_name.len() as u32;
            let unused_charters = (BYTEARRAY_LIMIT_NAME as u32) - length;
            let fee = FEE_PER_USED_CHAR * (unused_charters + 1) * (unused_charters + 1);
            Self::_pay_unique_name(sender.clone(), T::Balance::from(fee))?;

            let mut metalog = Self::meta_of_did(&did);
            metalog.name = unique_name.clone();

            let meta_index = <OwnedMetaIndex>::get(&did);
            <OwnedMetaArray<T>>::insert((sender.clone(), meta_index -1), &metalog);
            <DidMeta<T>>::insert(&did, &metalog);

            <UnMeta<T>>::insert(&metalog.name, &metalog);
            <UnOwner<T>>::insert(&metalog.name, &sender);

            Self::deposit_event(RawEvent::NameUpdated(sender, did, unique_name, T::Balance::from(fee)));
            Ok(())
        }

        // Change license code
        #[weight = 0]
        pub fn change_license_code(origin, did: Vec<u8>, license_code: u16)-> DispatchResult{
            let sender = ensure_signed(origin)?;

            Self::_check_did_ownership(sender.clone(), &did)?;
            let mut metadata = Self::meta_of_did(&did);
            metadata.code = license_code.clone();

            let meta_index = <OwnedMetaIndex>::get(&did);
            <OwnedMetaArray<T>>::insert((sender.clone(), meta_index -1), &metadata);
            <DidMeta<T>>::insert(&did, &metadata);

            Self::deposit_event(RawEvent::LicenseUpdated(sender, did, license_code));
            Ok(())
        }
    }
}

decl_event!(
	pub enum Event<T> where
        <T as frame_system::Trait>::AccountId,
        <T as timestamp::Trait>::Moment,
        <T as balances::Trait>::Balance
    {
        Stored(AccountId, Moment, Vec<u8>),
		TransferOwnership(AccountId, AccountId, Vec<u8>),
		LicenseUpdated(AccountId, Vec<u8>,u16),
		NameUpdated(AccountId, Vec<u8>,Vec<u8>, Balance),
	}
);

impl<T: Trait> Module<T> {
    /// store metalog
    fn _owner_store(sender: T::AccountId, metalog: Metalog<T::Moment>) -> DispatchResult {
        let count = Self::owner_meta_count(&sender);
        let updated_count = count.checked_add(1).ok_or(ERR_OVERFLOW)?;

        <OwnedMetaArray<T>>::insert((sender.clone(), count), &metalog);
        <OwnedMetaCount<T>>::insert(&sender, updated_count);
        <OwnedMetaIndex>::insert(&metalog.did, updated_count);

        <DidMeta<T>>::insert(&metalog.did, &metalog);
        <DidOwner<T>>::insert(&metalog.did, &sender);

        Ok(())
    }

    /// Checks the ownership rights
    fn _check_did_ownership(sender: T::AccountId, did: &Vec<u8>) -> DispatchResult {
        ensure!(<DidMeta<T>>::contains_key(did), ERR_DID_NOT_EXIST);
        let owner = Self::owner_of_did(did).ok_or(ERR_DID_NO_OWNER)?;
        ensure!(owner == sender, ERR_NOT_OWNER);

        Ok(())
    }

    /// Transfer ownership
    fn _transfer(sender: T::AccountId, receiver: T::AccountId, did: &Vec<u8>) -> DispatchResult {
        let receiver_total_count = Self::owner_meta_count(&receiver);
        let new_receiver_count = receiver_total_count.checked_add(1).ok_or(ERR_OVERFLOW)?;

        let sender_total_count = Self::owner_meta_count(&sender);
        let new_sender_count = sender_total_count.checked_sub(1).ok_or(ERR_UNDERFLOW)?;

        let meta_index = <OwnedMetaIndex>::get(did);
        let meta_object = <OwnedMetaArray<T>>::get((sender.clone(), new_sender_count));

        if meta_index != new_sender_count {
            <OwnedMetaArray<T>>::insert((sender.clone(), meta_index), &meta_object);
            <OwnedMetaIndex>::insert(&meta_object.did, meta_index);
        }

        // if un is not the default un
        let mut default_name = Vec::new();
        default_name.push(0);
        if meta_object.name != default_name {
            <UnOwner<T>>::insert(did, &receiver);
        }

        <DidOwner<T>>::insert(did, &receiver);

        <OwnedMetaIndex>::insert(did, receiver_total_count);

        <OwnedMetaArray<T>>::remove((sender.clone(), new_sender_count));
        <OwnedMetaArray<T>>::insert((receiver.clone(), receiver_total_count), meta_object);

        <OwnedMetaCount<T>>::insert(&sender, new_sender_count);
        <OwnedMetaCount<T>>::insert(&receiver, new_receiver_count);

        Ok(())
    }

    /// Payment for unique names
    fn _pay_unique_name(who: T::AccountId, fee: T::Balance) -> DispatchResult {
        let _ = <balances::Module<T> as Currency<_>>::withdraw(
            &who,
            fee,
            WithdrawReason::Fee.into(),
            ExistenceRequirement::KeepAlive,
        )?;
        Ok(())
    }
}



