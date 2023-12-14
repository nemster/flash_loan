use scrypto::prelude::*;

#[derive(ScryptoSbor, NonFungibleData)]
pub struct LenderData {
    initial_deposit: String,
    date: String,
    key_image_url: String,
    #[mutable]
    current_amount: Decimal,
}

#[derive(ScryptoSbor, NonFungibleData)]
pub struct TransientNftData {
    amount: Decimal
}

#[blueprint]
mod flash_loan {

    enable_method_auth! {
        roles {
            admin => updatable_by: [OWNER];
            bot => updatable_by: [OWNER];
            treasurer => updatable_by: [OWNER];
        },
        methods {
            add_funds => PUBLIC;
            withdraw_funds => PUBLIC;
            partial_withdraw => PUBLIC;
            get_loan => PUBLIC;
            return_loan => PUBLIC;
            set_borrower_fee => restrict_to: [admin];
            set_lender_rewards => restrict_to: [admin];
            withdraw_owner_rewards => restrict_to: [treasurer];
            distribute_rewards => restrict_to: [bot];
        }
    }

    struct FlashLoan {
        coins_to_lend: Vault,
        borrower_fee: Decimal,
        lender_rewards: Decimal,
        lender_resource_manager: ResourceManager,
        next_lender_id: u64,
        total_deposited: Decimal,
        transient_nft_resource_manager: ResourceManager,
        pending_rewards: Decimal
    }

    impl FlashLoan {

        pub fn instantiate_flash_loan(
            owner_badge_address: ResourceAddress,
            coins_address: ResourceAddress,
            borrower_percentage_fee: Decimal,
            lender_percentage_rewards: Decimal
        ) -> Global<FlashLoan> {

            let (address_reservation, component_address) =
                Runtime::allocate_component_address(FlashLoan::blueprint_id());

            let lender_resource_manager = ResourceBuilder::new_integer_non_fungible::<LenderData>(
                OwnerRole::Fixed(rule!(require(owner_badge_address)))
            )
                .metadata(metadata! {
                    init {
                        "name" => "FlashLoan lender badge", locked;
                        "icon_url" => MetadataValue::Url(UncheckedUrl::of("https://flash-loan.stakingcoins.eu/flash-loan.png")), locked;
                        "dapp_definitions" => "FlashLoan", locked;
                        "tags" => ["lending", "yeld"], locked;
                        "description" => "This is a receipt for your coins deposited in FlashLoan", locked;
                    }
                })
                .mint_roles(mint_roles!(
                    minter => rule!(require(global_caller(component_address)));
                    minter_updater => rule!(deny_all);
                ))
                .non_fungible_data_update_roles(non_fungible_data_update_roles!(
                    non_fungible_data_updater => rule!(require(global_caller(component_address)));
                    non_fungible_data_updater_updater => rule!(deny_all);
                ))
                .burn_roles(burn_roles!(
                    burner => rule!(require(global_caller(component_address)));
                    burner_updater => rule!(deny_all);
                ))
                .create_with_no_initial_supply();

            let transient_nft_resource_manager = ResourceBuilder::new_integer_non_fungible::<TransientNftData>(
                OwnerRole::Fixed(rule!(require(owner_badge_address)))
            )
                .metadata(metadata! {
                    init {
                        "name" => "FlashLoan transient NFT", locked;
                        "icon_url" => MetadataValue::Url(UncheckedUrl::of("https://flash-loan.stakingcoins.eu/flash-loan.png")), locked;
                        "dapp_definitions" => "FlashLoan", locked;
                    }
                })
                .mint_roles(mint_roles!(
                    minter => rule!(require(global_caller(component_address)));
                    minter_updater => rule!(deny_all);
                ))
                .burn_roles(burn_roles!(
                    burner => rule!(require(global_caller(component_address)));
                    burner_updater => rule!(deny_all);
                ))
                .deposit_roles(deposit_roles!(
                    depositor => rule!(deny_all);
                    depositor_updater => rule!(deny_all);
                ))
                .create_with_no_initial_supply();

            Self {
                coins_to_lend: Vault::new(coins_address),
                borrower_fee: borrower_percentage_fee,
                lender_rewards: lender_percentage_rewards,
                lender_resource_manager: lender_resource_manager,
                next_lender_id: 1,
                total_deposited: Decimal(I192::from(0)),
                transient_nft_resource_manager: transient_nft_resource_manager,
                pending_rewards: Decimal(I192::from(0)),
            }
                .instantiate()
                .prepare_to_globalize(OwnerRole::Fixed(rule!(require(owner_badge_address))))
                .roles(roles!(
                    admin => rule!(require(owner_badge_address));
                    bot => rule!(require(owner_badge_address));
                    treasurer => rule!(require(owner_badge_address));
                ))
                .metadata(metadata! {
                    init {
                        "name" => "FlashLoan", locked;
                        "icon_url" => MetadataValue::Url(UncheckedUrl::of("https://flash-loan.stakingcoins.eu/flash-loan.png")), locked;
                        "dapp_definitions" => "FlashLoan", locked;
                    }
                })
                .with_address(address_reservation)
                .globalize()
        }

        pub fn add_funds(&mut self, funds: Bucket) -> Bucket {
            let amount = funds.amount();

            let lender_local_id = NonFungibleLocalId::Integer(self.next_lender_id.into());

            let current_time = UtcDateTime::from_instant(&Clock::current_time_rounded_to_minutes()).unwrap();

            let nft = self.lender_resource_manager.mint_non_fungible(
                &lender_local_id,
                LenderData {
                    initial_deposit: amount.to_string(),
                    date: format!("{}-{}-{} {}:{}", current_time.year(), current_time.month(), current_time.day_of_month(), current_time.hour(), current_time.minute()),
                    key_image_url: "https://flash-loan.stakingcoins.eu/flash-loan.png".to_string(),
                    current_amount: amount
                }
            );

            self.next_lender_id += 1;
            self.coins_to_lend.put(funds);
            self.total_deposited += amount;

            return nft;
        }

        pub fn withdraw_funds(&mut self, lender_nft: Bucket) -> Bucket {
            assert!(lender_nft.resource_address() == self.lender_resource_manager.address(),
                    "Unknown badge");

            let mut amount = Decimal(I192::from(0));
            for nft in lender_nft.as_non_fungible().non_fungibles::<LenderData>() {
                amount += nft.data().current_amount;
            }
            self.total_deposited -= amount;

            lender_nft.burn();

            return self.coins_to_lend.take(amount);
        }

        pub fn partial_withdraw(&mut self, lender_nft: Bucket, percentage_withdrawed: Decimal) -> (Bucket, Bucket) {
            assert!(lender_nft.resource_address() == self.lender_resource_manager.address(),"Unknown badge"); 

            let mut amount = Decimal(I192::from(0));
            for nft in lender_nft.as_non_fungible().non_fungibles::<LenderData>() {
                let to_be_returned_amount = nft.data().current_amount * percentage_withdrawed / 100;
                info!("Calculating amount to be returned  {:?} ", to_be_returned_amount);  
                amount += to_be_returned_amount;
                info!("Updated amount to be returned  {:?} ", amount);  
            }
            self.total_deposited -= amount;
            info!("Returned amount {:?} ", amount);  

            for nft in lender_nft.as_non_fungible().non_fungibles::<LenderData>() {
                let to_be_returned_amount = nft.data().current_amount * percentage_withdrawed / 100;
                self.lender_resource_manager.update_non_fungible_data(&nft.local_id(), "current_amount", to_be_returned_amount );
            }

            return (self.coins_to_lend.take(amount), lender_nft);
        }

        pub fn get_loan(&mut self, amount: Decimal) -> (Bucket, Bucket) {
            let loan_bucket = self.coins_to_lend.take(amount);

            let local_id = NonFungibleLocalId::Integer(1.into());
            let nft_bucket = self.transient_nft_resource_manager.mint_non_fungible(
                &local_id,
                TransientNftData {
                    amount: amount
                }
            );

            return (loan_bucket, nft_bucket);
        }

        pub fn return_loan(&mut self, bucket: Bucket, transient_nft: Bucket) {
            assert!(transient_nft.resource_address() == self.transient_nft_resource_manager.address(), "Unknown badge");

            let loan_amount = transient_nft.as_non_fungible().non_fungible::<TransientNftData>().data().amount;
            assert!(bucket.amount() >= loan_amount * (100 + self.borrower_fee) / 100, "Insufficient amount");

            self.pending_rewards += loan_amount * self.lender_rewards / 100;

            self.coins_to_lend.put(bucket);
            
            transient_nft.burn();
        }

        pub fn set_borrower_fee(&mut self, percentage_fee: Decimal) {
            assert!(percentage_fee >= self.lender_rewards, "You're going broke");
            self.borrower_fee = percentage_fee;
        }

        pub fn set_lender_rewards(&mut self, percentage_rewards: Decimal) {
            assert!(self.borrower_fee >= percentage_rewards, "You're going broke");
            self.lender_rewards = percentage_rewards;
        }

        pub fn withdraw_owner_rewards(&mut self) -> Bucket {
            return self.coins_to_lend.take(self.coins_to_lend.amount() - self.total_deposited - self.pending_rewards);
        }

        pub fn distribute_rewards(&mut self) {
            let reward_per_coin = self.pending_rewards / self.total_deposited;
            self.total_deposited += self.pending_rewards;
            self.pending_rewards = Decimal(I192::from(0));

            for id in 1..self.next_lender_id {
                let local_id = NonFungibleLocalId::Integer(id.into());
                if self.lender_resource_manager.non_fungible_exists(&local_id) {
                    let current_amount = self.lender_resource_manager.get_non_fungible_data::<LenderData>(&local_id).current_amount;
                    self.lender_resource_manager.update_non_fungible_data(
                        &local_id,
                        "current_amount",
                        current_amount * (1 + reward_per_coin)
                    );
                }
            }
        }
    }
}
