#[cfg(test)]
mod tests {
    use crate::contract::instantiate;
    use crate::contract::{execute, reply};
    use crate::error::ContractError;
    use crate::mock_querier::mock_dependencies_custom;
    use crate::msg::ExecuteMsg;
    use crate::msg::InstantiateMsg;
    use crate::state::{Migration, STATE};
    use crate::state::{PollInfoState, PollStatus, Proposal, POLL};
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
    use cosmwasm_std::{coins, DepsMut, Uint128};
    use cosmwasm_std::{Coin, ContractResult, Decimal, Event, StdResult};

    struct BeforeAll {
        default_sender: String,
        default_sender_two: String,
        default_sender_owner: String,
    }
    fn before_all() -> BeforeAll {
        BeforeAll {
            default_sender: "addr0000".to_string(),
            default_sender_two: "addr0001".to_string(),
            default_sender_owner: "addr0002".to_string(),
        }
    }

    fn default_init(deps: DepsMut) {
        let msg = InstantiateMsg {
            code_id: 0,
            message: Default::default(),
            label: "".to_string(),
            staking_contract_address: "staking".to_string(),
            cw20_contract_address: "cw20".to_string(),
            poll_default_end_height: 0,
            required_amount: Uint128::new(100_000_000),
            denom: "uusd".to_string(),
        };
        let info = mock_info("creator", &coins(1000, "earth"));
        // we can just call .unwrap() to assert this was a success
        let _res = instantiate(deps, mock_env(), info, msg).unwrap();
    }
    #[test]
    fn proper_initialization() {
        let mut deps = mock_dependencies(&[]);

        let msg = InstantiateMsg {
            code_id: 0,
            message: Default::default(),
            label: "Hello world contract".to_string(),
            staking_contract_address: "staking".to_string(),
            cw20_contract_address: "cw20".to_string(),
            poll_default_end_height: 0,
            required_amount: Uint128::new(100_000_000),
            denom: "uusd".to_string(),
        };
        let info = mock_info("creator", &coins(1000, "earth"));

        // we can just call .unwrap() to assert this was a success
        let res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(1, res.messages.len());
    }

    mod proposal {
        use super::*;
        use cosmwasm_std::Api;

        // handle_proposal
        #[test]
        fn description_min_error() {
            let before_all = before_all();
            let mut deps = mock_dependencies(&[Coin {
                denom: "ust".to_string(),
                amount: Uint128::new(9_000_000),
            }]);
            default_init(deps.as_mut());
            let env = mock_env();
            let msg = ExecuteMsg::Poll {
                description: "This".to_string(),
                proposal: Proposal::LotteryEveryBlockTime,
                amount: Option::from(Uint128::new(22)),
                recipient: None,
                prizes_per_ranks: None,
                migration: None,
                contract_address: "lottery".to_string(),
            };
            let info = mock_info(
                before_all.default_sender.as_str().clone(),
                &[Coin {
                    denom: "uusd".to_string(),
                    amount: Uint128::new(100_000_000),
                }],
            );
            let res = execute(deps.as_mut(), env, info, msg);
            println!("{:?}", res);
            match res {
                Err(ContractError::WrongDescLength(4, 6, 255)) => {}
                _ => panic!("Unexpected error"),
            }
        }
        #[test]
        fn description_max_error() {
            let before_all = before_all();
            let mut deps = mock_dependencies(&[Coin {
                denom: "ust".to_string(),
                amount: Uint128::new(9_000_000),
            }]);
            default_init(deps.as_mut());
            let env = mock_env();
            let msg = ExecuteMsg::Poll {
                description: "let env = mock_env(before_all.default_sender.clone(), &[]);\
                 let env = mock_env(before_all.default_sender.clone(), &[]); let env \
                 = mock_env(before_all.default_sender.clone(), &[]); let env = mock_env(before_all.default_sender.clone(), &[]);\
                 let env = mock_env(before_all.default_sender.clone(), &[]);let env = mock_env(before_all.default_sender.clone(), &[]);
                 ".to_string(),
                proposal: Proposal::LotteryEveryBlockTime,
                amount: Option::from(Uint128::new(22)),
                recipient: None,
                prizes_per_ranks: None,
                migration: None,
                contract_address: "lottery".to_string()
            };
            let info = mock_info(
                before_all.default_sender.as_str().clone(),
                &[Coin {
                    denom: "uusd".to_string(),
                    amount: Uint128::new(100_000_000),
                }],
            );
            let res = execute(deps.as_mut(), env, info, msg);
            println!("{:?}", res);
            match res {
                Err(ContractError::WrongDescLength(374, 6, 255)) => {}
                _ => panic!("Unexpected error"),
            }
        }
        #[test]
        fn send_funds() {
            let before_all = before_all();
            let mut deps = mock_dependencies(&[Coin {
                denom: "ust".to_string(),
                amount: Uint128::new(9_000_000),
            }]);
            default_init(deps.as_mut());
            let env = mock_env();
            let msg = ExecuteMsg::Poll {
                description: "This is my first proposal".to_string(),
                proposal: Proposal::LotteryEveryBlockTime,
                amount: Option::from(Uint128::new(22)),
                recipient: None,
                prizes_per_ranks: None,
                migration: None,
                contract_address: "lottery".to_string(),
            };
            let info = mock_info(before_all.default_sender.as_str().clone(), &[]);
            let res = execute(deps.as_mut(), env, info, msg);
            println!("{:?}", res);
            let _expected = Uint128::new(100000000);
            match res {
                Err(ContractError::RequiredCollateral(_expected)) => {}
                _ => panic!("Unexpected error"),
            }
        }

        fn msg_constructor_none(proposal: Proposal) -> ExecuteMsg {
            ExecuteMsg::Poll {
                description: "This is my first proposal".to_string(),
                proposal,
                amount: None,
                recipient: None,
                prizes_per_ranks: None,
                migration: None,
                contract_address: "lottery".to_string(),
            }
        }
        fn msg_constructor_amount_out(proposal: Proposal) -> ExecuteMsg {
            ExecuteMsg::Poll {
                description: "This is my first proposal".to_string(),
                proposal,
                amount: Option::from(Uint128::new(250)),
                recipient: None,
                prizes_per_ranks: None,
                migration: None,
                contract_address: "lottery".to_string(),
            }
        }

        fn msg_constructor_prize_len_out(proposal: Proposal) -> ExecuteMsg {
            ExecuteMsg::Poll {
                description: "This is my first proposal".to_string(),
                proposal,
                amount: None,
                recipient: None,
                prizes_per_ranks: Option::from(vec![100, 200, 230, 230, 230, 230, 234]),
                migration: None,
                contract_address: "lottery".to_string(),
            }
        }

        fn msg_constructor_prize_sum_out(proposal: Proposal) -> ExecuteMsg {
            ExecuteMsg::Poll {
                description: "This is my first proposal".to_string(),
                proposal,
                amount: None,
                recipient: None,
                prizes_per_ranks: Option::from(vec![1000, 200, 230, 230, 0, 0]),
                migration: None,
                contract_address: "lottery".to_string(),
            }
        }

        #[test]
        fn all_proposal_amount_error() {
            let before_all = before_all();
            let mut deps = mock_dependencies(&[Coin {
                denom: "ust".to_string(),
                amount: Uint128::new(9_000_000),
            }]);
            default_init(deps.as_mut());
            let env = mock_env();

            let msg_drand_worker_fee_percentage =
                msg_constructor_none(Proposal::DrandWorkerFeePercentage);
            let msg_lottery_every_block_time =
                msg_constructor_none(Proposal::LotteryEveryBlockTime);
            let msg_jackpot_reward_percentage =
                msg_constructor_none(Proposal::JackpotRewardPercentage);
            let msg_prize_per_rank = msg_constructor_none(Proposal::PrizesPerRanks);
            let msg_holder_fee_per_percentage = msg_constructor_none(Proposal::HolderFeePercentage);
            let msg_amount_to_register = msg_constructor_none(Proposal::AmountToRegister);
            let msg_security_migration = msg_constructor_none(Proposal::SecurityMigration);
            let msg_dao_funding = msg_constructor_none(Proposal::DaoFunding);
            let msg_staking_contract_migration =
                msg_constructor_none(Proposal::StakingContractMigration);

            let info = mock_info(
                before_all.default_sender.as_str().clone(),
                &[Coin {
                    denom: "uusd".to_string(),
                    amount: Uint128::new(100_000_000),
                }],
            );
            let res = execute(deps.as_mut(), env.clone(), info.clone(), msg_dao_funding);
            match res {
                Err(ContractError::InvalidAmount {}) => {}
                _ => panic!("unexpected"),
            }

            let res = execute(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                msg_security_migration,
            );
            match res {
                Err(ContractError::InvalidMigration()) => {}
                _ => panic!("Unexpected error"),
            }

            let res = execute(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                msg_staking_contract_migration,
            );
            match res {
                Err(ContractError::NoMigrationAddress()) => {}
                _ => panic!("Unexpected error"),
            }

            let res = execute(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                msg_lottery_every_block_time,
            );
            match res {
                Err(ContractError::InvalidBlockTime {}) => {}
                _ => panic!("Unexpected error"),
            }

            let res = execute(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                msg_drand_worker_fee_percentage,
            );
            match res {
                Err(ContractError::InvalidAmount {}) => {}
                _ => panic!("Unexpected error"),
            }

            let res = execute(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                msg_jackpot_reward_percentage,
            );
            match res {
                Err(ContractError::InvalidAmount {}) => {}
                _ => panic!("Unexpected error"),
            }

            let res = execute(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                msg_holder_fee_per_percentage,
            );
            match res {
                Err(ContractError::InvalidAmount {}) => {}
                _ => panic!("Unexpected error"),
            }

            let res = execute(deps.as_mut(), env.clone(), info.clone(), msg_prize_per_rank);
            match res {
                Err(ContractError::InvalidRank {}) => {}
                _ => panic!("Unexpected error"),
            }

            let res = execute(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                msg_amount_to_register,
            );
            match res {
                Err(ContractError::InvalidAmount {}) => {}
                _ => panic!("Unexpected error"),
            }

            let msg_drand_worker_fee_percentage =
                msg_constructor_amount_out(Proposal::DrandWorkerFeePercentage);
            let msg_jackpot_reward_percentage =
                msg_constructor_amount_out(Proposal::JackpotRewardPercentage);
            let msg_holder_fee_per_percentage =
                msg_constructor_amount_out(Proposal::HolderFeePercentage);

            let res = execute(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                msg_drand_worker_fee_percentage,
            );
            println!("{:?}", res);
            match res {
                Err(ContractError::MaxReward(10)) => {}
                _ => {
                    panic!("Unexpected error")
                }
            }

            let res = execute(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                msg_jackpot_reward_percentage,
            );
            println!("{:?}", res);
            match res {
                Err(ContractError::InvalidAmount()) => {}
                _ => panic!("Unexpected error"),
            }

            let res = execute(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                msg_holder_fee_per_percentage,
            );
            println!("{:?}", res);
            match res {
                Err(ContractError::MaxReward(20)) => {}
                _ => {
                    panic!("Unexpected error")
                }
            }

            let msg_prize_per_rank = msg_constructor_prize_len_out(Proposal::PrizesPerRanks);
            let res = execute(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                msg_prize_per_rank.clone(),
            );
            match res {
                Err(ContractError::InvalidRank()) => {}
                _ => panic!("Unexpected error"),
            }
            let msg_prize_per_rank = msg_constructor_prize_sum_out(Proposal::PrizesPerRanks);
            let res = execute(deps.as_mut(), env.clone(), info.clone(), msg_prize_per_rank);
            match res {
                Err(ContractError::InvalidNumberSum()) => {}
                _ => panic!("Unexpected error"),
            }
        }
        fn msg_constructor_success(
            proposal: Proposal,
            amount: Option<Uint128>,
            prizes_per_ranks: Option<Vec<u64>>,
            recipient: Option<String>,
            migration: Option<Migration>,
        ) -> ExecuteMsg {
            ExecuteMsg::Poll {
                description: "This is my first proposal".to_string(),
                proposal,
                amount,
                recipient,
                prizes_per_ranks,
                migration,
                contract_address: "lottery".to_string(),
            }
        }

        #[test]
        fn success() {
            let before_all = before_all();
            let mut deps = mock_dependencies_custom(&[Coin {
                denom: "ust".to_string(),
                amount: Uint128::new(9_000_000),
            }]);
            deps.querier.with_token_balances(Uint128::new(200_000));
            default_init(deps.as_mut());
            let state = STATE.load(deps.as_ref().storage).unwrap();
            assert_eq!(state.poll_id, 0);
            let env = mock_env();
            let info = mock_info(
                before_all.default_sender.as_str().clone(),
                &[Coin {
                    denom: "uusd".to_string(),
                    amount: Uint128::new(100_000_000),
                }],
            );
            let msg_lottery_every_block_time = msg_constructor_success(
                Proposal::LotteryEveryBlockTime,
                Option::from(Uint128::new(22)),
                None,
                None,
                None,
            );
            let msg_amount_to_register = msg_constructor_success(
                Proposal::AmountToRegister,
                Option::from(Uint128::new(22)),
                None,
                None,
                None,
            );
            let msg_holder_fee_percentage = msg_constructor_success(
                Proposal::HolderFeePercentage,
                Option::from(Uint128::new(20)),
                None,
                None,
                None,
            );
            let msg_prize_rank = msg_constructor_success(
                Proposal::PrizesPerRanks,
                None,
                Option::from(vec![100, 100, 100, 700, 0, 0]),
                None,
                None,
            );
            let msg_jackpot_reward_percentage = msg_constructor_success(
                Proposal::JackpotRewardPercentage,
                Option::from(Uint128::new(80)),
                None,
                None,
                None,
            );
            let msg_drand_fee_worker = msg_constructor_success(
                Proposal::DrandWorkerFeePercentage,
                Option::from(Uint128::new(10)),
                None,
                None,
                None,
            );
            let msg_security_migration = msg_constructor_success(
                Proposal::SecurityMigration,
                None,
                None,
                Some(before_all.default_sender_two.clone()),
                Some(Migration {
                    contract_addr: "new".to_string(),
                    new_code_id: 1,
                    msg: Default::default(),
                }),
            );
            let msg_dao_funding = msg_constructor_success(
                Proposal::DaoFunding,
                Option::from(Uint128::new(200_000)),
                None,
                None,
                None,
            );

            let msg_staking_contract_migration = msg_constructor_success(
                Proposal::StakingContractMigration,
                None,
                None,
                Option::from(before_all.default_sender_two.clone()),
                None,
            );

            let res = execute(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                msg_lottery_every_block_time,
            )
            .unwrap();
            assert_eq!(res.attributes.len(), 3);
            let poll_state = POLL
                .load(deps.as_ref().storage, &1_u64.to_be_bytes())
                .unwrap();
            assert_eq!(
                poll_state.creator,
                deps.api
                    .addr_canonicalize(&before_all.default_sender)
                    .unwrap()
            );
            let state = STATE.load(deps.as_ref().storage).unwrap();
            assert_eq!(state.poll_id, 1);

            let res = execute(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                msg_amount_to_register,
            )
            .unwrap();
            assert_eq!(res.attributes.len(), 3);
            let res = execute(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                msg_holder_fee_percentage,
            )
            .unwrap();
            assert_eq!(res.attributes.len(), 3);
            let res = execute(deps.as_mut(), env.clone(), info.clone(), msg_prize_rank).unwrap();
            assert_eq!(res.attributes.len(), 3);
            let res = execute(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                msg_jackpot_reward_percentage,
            )
            .unwrap();
            assert_eq!(res.attributes.len(), 3);
            let res = execute(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                msg_drand_fee_worker,
            )
            .unwrap();
            assert_eq!(res.attributes.len(), 3);

            // Admin create proposal migration
            let env = mock_env();
            let info = mock_info(
                before_all.default_sender_owner.as_str().clone(),
                &[Coin {
                    denom: "uusd".to_string(),
                    amount: Uint128::new(100_000_000),
                }],
            );
            let res = execute(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                msg_security_migration.clone(),
            )
            .unwrap();
            assert_eq!(res.attributes.len(), 3);
            let res = execute(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                msg_staking_contract_migration.clone(),
            )
            .unwrap();
            assert_eq!(res.attributes.len(), 3);
            let res = execute(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                msg_dao_funding.clone(),
            )
            .unwrap();
            assert_eq!(res.attributes.len(), 3);

            let env = mock_env();
            let info = mock_info(
                before_all.default_sender.as_str().clone(),
                &[Coin {
                    denom: "uusd".to_string(),
                    amount: Uint128::new(100_000_000),
                }],
            );
            let res = execute(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                msg_security_migration.clone(),
            )
            .unwrap();
            assert_eq!(res.attributes.len(), 3);
            let res = execute(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                msg_staking_contract_migration.clone(),
            )
            .unwrap();
            assert_eq!(res.attributes.len(), 3);
            let res = execute(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                msg_dao_funding.clone(),
            )
            .unwrap();
            assert_eq!(res.attributes.len(), 3);
        }
    }
    mod vote {
        use super::*;
        use crate::contract::execute;
        use crate::msg::ExecuteMsg;
        use crate::state::{PollInfoState, PollStatus, Proposal, POLL, POLL_VOTE};
        use cosmwasm_std::{Api, Coin, Decimal, StdError, StdResult};

        // handle_vote
        fn create_poll(deps: DepsMut) {
            let msg = ExecuteMsg::Poll {
                description: "This is my first proposal".to_string(),
                proposal: Proposal::LotteryEveryBlockTime,
                amount: Option::from(Uint128::new(22)),
                prizes_per_ranks: None,
                recipient: None,
                migration: None,
                contract_address: "lottery".to_string(),
            };

            let _res = execute(
                deps,
                mock_env(),
                mock_info(
                    "addr0000",
                    &[Coin {
                        denom: "uusd".to_string(),
                        amount: Uint128::new(100),
                    }],
                ),
                msg,
            )
            .unwrap();
        }
        #[test]
        fn do_not_send_funds() {
            let before_all = before_all();
            let mut deps = mock_dependencies(&[Coin {
                denom: "ust".to_string(),
                amount: Uint128::new(9_000_000),
            }]);
            default_init(deps.as_mut());
            create_poll(deps.as_mut());

            let env = mock_env();
            let info = mock_info(
                before_all.default_sender.as_str().clone(),
                &[Coin {
                    denom: "ust".to_string(),
                    amount: Uint128::new(9_000_000),
                }],
            );
            let msg = ExecuteMsg::Vote {
                poll_id: 1,
                approve: false,
            };
            let res = execute(deps.as_mut(), env, info, msg);
            println!("{:?}", res);
            match res {
                Err(ContractError::DoNotSendFunds {}) => {}
                _ => panic!("Unexpected error"),
            }
        }
        #[test]
        fn poll_deactivated() {
            let before_all = before_all();
            let mut deps = mock_dependencies(&[Coin {
                denom: "ust".to_string(),
                amount: Uint128::new(9_000_000),
            }]);
            default_init(deps.as_mut());
            create_poll(deps.as_mut());

            // Save to storage
            POLL.update(
                deps.as_mut().storage,
                &1u64.to_be_bytes(),
                |poll| -> StdResult<PollInfoState> {
                    match poll {
                        None => Err(StdError::generic_err("error")),
                        Some(poll_state) => {
                            let mut poll_data = poll_state;
                            // Update the status to passed
                            poll_data.status = PollStatus::RejectedByCreator;
                            Ok(poll_data)
                        }
                    }
                },
            )
            .unwrap();

            let env = mock_env();
            let info = mock_info(before_all.default_sender.as_str().clone(), &[]);
            let msg = ExecuteMsg::Vote {
                poll_id: 1,
                approve: false,
            };
            let res = execute(deps.as_mut(), env, info, msg);
            println!("{:?}", res);
            match res {
                Err(ContractError::PollClosed {}) => {}
                _ => panic!("Unexpected error"),
            }
        }
        #[test]
        fn poll_expired() {
            let before_all = before_all();
            let mut deps = mock_dependencies(&[Coin {
                denom: "ust".to_string(),
                amount: Uint128::new(9_000_000),
            }]);
            default_init(deps.as_mut());
            create_poll(deps.as_mut());

            let mut env = mock_env();
            let info = mock_info(before_all.default_sender.as_str().clone(), &[]);
            let poll_state = POLL
                .load(deps.as_ref().storage, &1_u64.to_be_bytes())
                .unwrap();
            env.block.height = poll_state.end_height + 1;

            let msg = ExecuteMsg::Vote {
                poll_id: 1,
                approve: false,
            };
            let res = execute(deps.as_mut(), env, info, msg);
            println!("{:?}", res);
            match res {
                Err(ContractError::PollExpired {}) => {}
                _ => panic!("Unexpected error"),
            }
        }
        #[test]
        fn only_stakers_with_bonded_tokens_can_vote() {
            let before_all = before_all();
            let mut deps = mock_dependencies_custom(&[Coin {
                denom: "ust".to_string(),
                amount: Uint128::new(9_000_000),
            }]);
            deps.querier.with_holder(
                before_all.default_sender.clone(),
                Uint128::new(0),
                Decimal::zero(),
                Decimal::zero(),
            );

            default_init(deps.as_mut());
            create_poll(deps.as_mut());

            let env = mock_env();
            let info = mock_info(before_all.default_sender.as_str().clone(), &[]);
            let msg = ExecuteMsg::Vote {
                poll_id: 1,
                approve: false,
            };
            let res = execute(deps.as_mut(), env, info, msg);
            match res {
                Err(ContractError::OnlyStakersVote {}) => {}
                _ => panic!("Unexpected error"),
            }
        }
        #[test]
        fn success() {
            let before_all = before_all();
            let mut deps = mock_dependencies_custom(&[Coin {
                denom: "ust".to_string(),
                amount: Uint128::new(9_000_000),
            }]);
            deps.querier.with_holder(
                before_all.default_sender.clone(),
                Uint128::new(150_000),
                Decimal::zero(),
                Decimal::zero(),
            );
            default_init(deps.as_mut());
            create_poll(deps.as_mut());

            let env = mock_env();
            let info = mock_info(before_all.default_sender.as_str(), &[]);
            let poll_id: u64 = 1;
            let approve = false;
            let msg = ExecuteMsg::Vote { poll_id, approve };
            let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone()).unwrap();
            let poll_state = POLL
                .load(deps.as_ref().storage, &poll_id.to_be_bytes())
                .unwrap();
            assert_eq!(res.attributes.len(), 3);
            assert_eq!(poll_state.no_vote, 1);
            assert_eq!(poll_state.yes_vote, 0);
            assert_eq!(poll_state.weight_yes_vote, Uint128::zero());
            assert_eq!(poll_state.weight_no_vote, Uint128::new(150_000));

            let sender_to_canonical = deps
                .api
                .addr_canonicalize(&before_all.default_sender)
                .unwrap();
            let vote_state = POLL_VOTE
                .load(
                    deps.as_ref().storage,
                    (
                        &poll_id.to_be_bytes().clone(),
                        sender_to_canonical.as_slice(),
                    ),
                )
                .unwrap();
            assert_eq!(vote_state, approve);

            // Try to vote multiple times
            let res = execute(deps.as_mut(), env, info, msg);
            match res {
                Err(ContractError::AlreadyVoted {}) => {}
                _ => panic!("Unexpected error"),
            }
        }
    }
    mod reject {
        use super::*;
        use crate::contract::execute;
        use crate::msg::ExecuteMsg;
        use crate::state::POLL;
        use cosmwasm_std::{Coin, Event, Reply, SubMsgExecutionResponse};

        // handle_reject
        fn create_poll(deps: DepsMut) {
            let msg = ExecuteMsg::Poll {
                description: "This is my first proposal".to_string(),
                proposal: Proposal::LotteryEveryBlockTime,
                amount: Option::from(Uint128::new(22)),
                recipient: None,
                prizes_per_ranks: None,
                migration: None,
                contract_address: "lottery".to_string(),
            };
            let _res = execute(
                deps,
                mock_env(),
                mock_info(
                    "addr0000",
                    &[Coin {
                        denom: "uusd".to_string(),
                        amount: Uint128::new(100_000_000),
                    }],
                ),
                msg,
            )
            .unwrap();
        }
        #[test]
        fn do_not_send_funds() {
            let before_all = before_all();
            let mut deps = mock_dependencies(&[Coin {
                denom: "ust".to_string(),
                amount: Uint128::new(9_000_000),
            }]);
            default_init(deps.as_mut());
            create_poll(deps.as_mut());
            let env = mock_env();
            let info = mock_info(
                before_all.default_sender.as_str().clone(),
                &[Coin {
                    denom: "ust".to_string(),
                    amount: Uint128::new(1_000),
                }],
            );
            let msg = ExecuteMsg::RejectPoll { poll_id: 1 };
            let res = execute(deps.as_mut(), env, info, msg);
            println!("{:?}", res);
            match res {
                Err(ContractError::DoNotSendFunds {}) => {}
                _ => panic!("Unexpected error"),
            }
        }
        #[test]
        fn poll_expired() {
            let before_all = before_all();
            let mut deps = mock_dependencies(&[Coin {
                denom: "ust".to_string(),
                amount: Uint128::new(9_000_000),
            }]);
            default_init(deps.as_mut());
            create_poll(deps.as_mut());
            let mut env = mock_env();
            let info = mock_info(before_all.default_sender.as_str().clone(), &[]);
            let poll_state = POLL
                .load(deps.as_ref().storage, &1_u64.to_be_bytes())
                .unwrap();
            env.block.height = poll_state.end_height + 1;
            let msg = ExecuteMsg::RejectPoll { poll_id: 1 };
            let res = execute(deps.as_mut(), env, info, msg);
            println!("{:?}", res);
            match res {
                Err(ContractError::ProposalExpired {}) => {}
                _ => panic!("Unexpected error"),
            }
        }
        #[test]
        fn only_creator_can_reject() {
            let before_all = before_all();
            let mut deps = mock_dependencies(&[Coin {
                denom: "ust".to_string(),
                amount: Uint128::new(9_000_000),
            }]);
            default_init(deps.as_mut());
            create_poll(deps.as_mut());
            let msg = ExecuteMsg::RejectPoll { poll_id: 1 };
            let env = mock_env();
            let info = mock_info(before_all.default_sender_two.as_str().clone(), &[]);
            let res = execute(deps.as_mut(), env, info, msg);

            println!("{:?}", res);
            match res {
                Err(ContractError::Unauthorized {}) => {}
                _ => panic!("Unexpected error"),
            }
        }
        #[test]
        fn success() {
            let before_all = before_all();
            let mut deps = mock_dependencies_custom(&[Coin {
                denom: "ust".to_string(),
                amount: Uint128::new(9_000_000),
            }]);
            default_init(deps.as_mut());
            create_poll(deps.as_mut());
            let event =
                Event::new("instantiate_contract").add_attribute("contract_address", "loterra");
            let result = ContractResult::Ok(SubMsgExecutionResponse {
                events: vec![event],
                data: None,
            });
            let _reply = reply(deps.as_mut(), mock_env(), Reply { id: 0, result }).unwrap();
            let msg = ExecuteMsg::RejectPoll { poll_id: 1 };
            let info = mock_info(before_all.default_sender.as_str().clone(), &[]);
            let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
            println!("{:?}", res);
            assert_eq!(res.messages.len(), 1);
            assert_eq!(res.attributes.len(), 2);
            let poll_state = POLL
                .load(deps.as_ref().storage, &1_u64.to_be_bytes())
                .unwrap();
            assert_eq!(poll_state.status, PollStatus::RejectedByCreator);
        }
    }

    mod present {
        use super::*;
        use cosmwasm_std::{Reply, SubMsgExecutionResponse};

        // handle_present
        fn create_poll(deps: DepsMut) {
            let msg = ExecuteMsg::Poll {
                description: "This is my first proposal".to_string(),
                proposal: Proposal::LotteryEveryBlockTime,
                amount: Some(Uint128::new(22)),
                recipient: None,
                prizes_per_ranks: None,
                migration: None,
                contract_address: "lottery".to_string(),
            };
            let _res = execute(
                deps,
                mock_env(),
                mock_info(
                    "addr0000",
                    &[Coin {
                        denom: "uusd".to_string(),
                        amount: Uint128::new(100_000_000),
                    }],
                ),
                msg,
            )
            .unwrap();
        }
        fn create_poll_security_migration(deps: DepsMut) {
            let msg = ExecuteMsg::Poll {
                description: "This is my first proposal".to_string(),
                proposal: Proposal::SecurityMigration,
                amount: None,
                recipient: Some("newAddress".to_string()),
                prizes_per_ranks: None,
                migration: Some(Migration {
                    contract_addr: "new".to_string(),
                    new_code_id: 0,
                    msg: Default::default(),
                }),
                contract_address: "lottery".to_string(),
            };
            let _res = execute(
                deps,
                mock_env(),
                mock_info(
                    "addr0002",
                    &[Coin {
                        denom: "uusd".to_string(),
                        amount: Uint128::new(100_000_000),
                    }],
                ),
                msg,
            )
            .unwrap();
            println!("{:?}", _res);
        }
        fn create_poll_dao_funding(deps: DepsMut) {
            let msg = ExecuteMsg::Poll {
                description: "This is my first proposal".to_string(),
                proposal: Proposal::DaoFunding,
                amount: Some(Uint128::new(22)),
                recipient: None,
                prizes_per_ranks: None,
                migration: None,
                contract_address: "lottery".to_string(),
            };
            let _res = execute(
                deps,
                mock_env(),
                mock_info(
                    "addr0002",
                    &[Coin {
                        denom: "uusd".to_string(),
                        amount: Uint128::new(100_000_000),
                    }],
                ),
                msg,
            )
            .unwrap();
        }
        fn create_poll_statking_contract_migration(deps: DepsMut) {
            let msg = ExecuteMsg::Poll {
                description: "This is my first proposal".to_string(),
                proposal: Proposal::StakingContractMigration,
                amount: None,
                recipient: Some("newAddress".to_string()),
                prizes_per_ranks: None,
                migration: None,
                contract_address: "lottery".to_string(),
            };
            let _res = execute(
                deps,
                mock_env(),
                mock_info(
                    "addr0002",
                    &[Coin {
                        denom: "uusd".to_string(),
                        amount: Uint128::new(100_000_000),
                    }],
                ),
                msg,
            )
            .unwrap();
            println!("{:?}", _res);
        }
        #[test]
        fn do_not_send_funds() {
            let before_all = before_all();
            let mut deps = mock_dependencies(&[Coin {
                denom: "ust".to_string(),
                amount: Uint128::new(9_000_000),
            }]);
            default_init(deps.as_mut());
            create_poll(deps.as_mut());

            let env = mock_env();
            let info = mock_info(
                before_all.default_sender.as_str().clone(),
                &[Coin {
                    denom: "ust".to_string(),
                    amount: Uint128::new(9_000_000),
                }],
            );
            let msg = ExecuteMsg::PresentPoll { poll_id: 1 };
            let res = execute(deps.as_mut(), env, info, msg);
            println!("{:?}", res);
            match res {
                Err(ContractError::DoNotSendFunds {}) => {}
                _ => panic!("Unexpected error"),
            }
        }
        #[test]
        fn poll_expired() {
            let before_all = before_all();
            let mut deps = mock_dependencies(&[Coin {
                denom: "ust".to_string(),
                amount: Uint128::new(9_000_000),
            }]);
            default_init(deps.as_mut());
            create_poll(deps.as_mut());
            // Save to storage

            POLL.update(
                deps.as_mut().storage,
                &1_u64.to_be_bytes(),
                |poll| -> StdResult<PollInfoState> {
                    match poll {
                        None => panic!("Unexpected error"),
                        Some(poll_state) => {
                            let mut poll_data = poll_state;
                            // Update the status to passed
                            poll_data.status = PollStatus::Rejected;
                            Ok(poll_data)
                        }
                    }
                },
            )
            .unwrap();
            let env = mock_env();
            let info = mock_info(before_all.default_sender.as_str().clone(), &[]);
            let msg = ExecuteMsg::PresentPoll { poll_id: 1 };
            let res = execute(deps.as_mut(), env, info, msg);
            println!("{:?}", res);
            match res {
                Err(ContractError::Unauthorized {}) => {}
                _ => panic!("Unexpected error"),
            }
        }
        #[test]
        fn poll_still_in_progress() {
            let before_all = before_all();
            let mut deps = mock_dependencies_custom(&[Coin {
                denom: "ust".to_string(),
                amount: Uint128::new(9_000_000),
            }]);

            default_init(deps.as_mut());
            create_poll(deps.as_mut());

            let env = mock_env();
            let info = mock_info(before_all.default_sender.as_str().clone(), &[]);
            let msg = ExecuteMsg::PresentPoll { poll_id: 1 };
            POLL.update(
                deps.as_mut().storage,
                &1_u64.to_be_bytes(),
                |poll| -> StdResult<PollInfoState> {
                    match poll {
                        None => panic!("Unexpected error"),
                        Some(poll_state) => {
                            let mut poll_data = poll_state;
                            // Update the status to passed
                            poll_data.end_height = env.block.height + 100;
                            Ok(poll_data)
                        }
                    }
                },
            )
            .unwrap();
            let res = execute(deps.as_mut(), env, info, msg);
            println!("{:?}", res);
            match res {
                Err(ContractError::ProposalInProgress {}) => {}
                _ => panic!("Unexpected error"),
            }
        }
        #[test]
        fn success_with_reject() {
            let before_all = before_all();
            let mut deps = mock_dependencies_custom(&[Coin {
                denom: "ust".to_string(),
                amount: Uint128::new(9_000_000),
            }]);
            deps.querier.with_token_balances(Uint128::new(200_000));
            default_init(deps.as_mut());
            create_poll(deps.as_mut());

            let mut env = mock_env();
            let info = mock_info(before_all.default_sender.as_str().clone(), &[]);
            let poll_state = POLL
                .load(deps.as_ref().storage, &1_u64.to_be_bytes())
                .unwrap();
            env.block.height = poll_state.end_height + 1;

            let msg = ExecuteMsg::PresentPoll { poll_id: 1 };
            let res = execute(deps.as_mut(), env, info, msg).unwrap();
            assert_eq!(res.attributes.len(), 3);
            assert_eq!(res.messages.len(), 0);

            let poll_state = POLL
                .load(deps.as_ref().storage, &1_u64.to_be_bytes())
                .unwrap();
            assert_eq!(poll_state.status, PollStatus::Rejected);
        }

        #[test]
        fn success_with_passed() {
            let before_all = before_all();
            let mut deps = mock_dependencies_custom(&[Coin {
                denom: "ust".to_string(),
                amount: Uint128::new(9_000_000),
            }]);
            //deps.querier.with_token_balances(Uint128::new(200_000));
            deps.querier.with_holder(
                before_all.default_sender.clone(),
                Uint128::new(100_000_000),
                Decimal::zero(),
                Decimal::zero(),
            );
            default_init(deps.as_mut());
            let _env = mock_env();
            let _info = mock_info(before_all.default_sender.as_str().clone(), &[]);
            create_poll(deps.as_mut());

            let env = mock_env();
            let info = mock_info(before_all.default_sender.as_str().clone(), &[]);
            let msg = ExecuteMsg::Vote {
                poll_id: 1,
                approve: true,
            };

            let _res = execute(deps.as_mut(), env, info, msg);

            let mut env = mock_env();
            let info = mock_info(before_all.default_sender.as_str().clone(), &[]);
            let poll_state = POLL
                .load(deps.as_ref().storage, &1_u64.to_be_bytes())
                .unwrap();
            env.block.height = poll_state.end_height + 1;

            let msg = ExecuteMsg::PresentPoll { poll_id: 1 };
            let event =
                Event::new("instantiate_contract").add_attribute("contract_address", "loterra");
            let result = ContractResult::Ok(SubMsgExecutionResponse {
                events: vec![event],
                data: None,
            });
            let _reply = reply(deps.as_mut(), env.clone(), Reply { id: 0, result }).unwrap();
            let res = execute(deps.as_mut(), env, info, msg).unwrap();
            assert_eq!(res.attributes.len(), 3);
            assert_eq!(res.messages.len(), 2);

            let poll_state = POLL
                .load(deps.as_ref().storage, &1_u64.to_be_bytes())
                .unwrap();
            assert_eq!(poll_state.status, PollStatus::Passed);

            let env = mock_env();
            let info = mock_info(before_all.default_sender.as_str().clone(), &[]);
            create_poll_security_migration(deps.as_mut());
            let msg = ExecuteMsg::Vote {
                poll_id: 2,
                approve: true,
            };
            let res = execute(deps.as_mut(), env, info, msg).unwrap();
            println!("{:?}", res);
        }
        #[test]
        fn success_with_proposal_not_expired_yet_and_more_50_percent_weight_vote() {
            let before_all = before_all();
            let mut deps = mock_dependencies_custom(&[Coin {
                denom: "ust".to_string(),
                amount: Uint128::new(9_000_000),
            }]);
            // deps.querier.with_token_balances(Uint128::new(200_000));
            deps.querier.with_holder(
                before_all.default_sender.clone(),
                Uint128::new(500_000_000),
                Decimal::zero(),
                Decimal::zero(),
            );
            default_init(deps.as_mut());
            let env = mock_env();
            let _info = mock_info(before_all.default_sender.as_str().clone(), &[]);
            create_poll(deps.as_mut());

            let event =
                Event::new("instantiate_contract").add_attribute("contract_address", "loterra");
            let result = ContractResult::Ok(SubMsgExecutionResponse {
                events: vec![event],
                data: None,
            });
            let _reply = reply(deps.as_mut(), env.clone(), Reply { id: 0, result }).unwrap();
            let env = mock_env();
            let info = mock_info(before_all.default_sender.as_str().clone(), &[]);
            let msg = ExecuteMsg::Vote {
                poll_id: 1,
                approve: true,
            };

            let _res = execute(deps.as_mut(), env, info, msg);

            let mut env = mock_env();
            let info = mock_info(before_all.default_sender.as_str().clone(), &[]);
            let poll_state = POLL
                .load(deps.as_ref().storage, &1_u64.to_be_bytes())
                .unwrap();
            env.block.height = poll_state.end_height - 1000;

            let msg = ExecuteMsg::PresentPoll { poll_id: 1 };
            let res = execute(deps.as_mut(), env, info, msg).unwrap();
            assert_eq!(res.attributes.len(), 3);
            assert_eq!(res.messages.len(), 2);

            let poll_state = POLL
                .load(deps.as_ref().storage, &1_u64.to_be_bytes())
                .unwrap();
            assert_eq!(poll_state.status, PollStatus::Passed);

            let env = mock_env();
            let info = mock_info(before_all.default_sender.as_str().clone(), &[]);
            create_poll_security_migration(deps.as_mut());
            let msg = ExecuteMsg::Vote {
                poll_id: 2,
                approve: true,
            };
            let res = execute(deps.as_mut(), env, info, msg).unwrap();
            println!("{:?}", res);
        }

        #[test]
        fn error_with_proposal_not_expired_yet_and_less_50_percent_weight_vote() {
            let before_all = before_all();
            let mut deps = mock_dependencies_custom(&[Coin {
                denom: "ust".to_string(),
                amount: Uint128::new(9_000_000),
            }]);
            deps.querier.with_token_balances(Uint128::new(200_000));
            deps.querier.with_holder(
                before_all.default_sender.clone(),
                Uint128::new(1_000),
                Decimal::zero(),
                Decimal::zero(),
            );
            default_init(deps.as_mut());
            let _env = mock_env();
            let _info = mock_info(before_all.default_sender.as_str().clone(), &[]);
            create_poll(deps.as_mut());

            let env = mock_env();
            let info = mock_info(before_all.default_sender.as_str().clone(), &[]);
            let msg = ExecuteMsg::Vote {
                poll_id: 1,
                approve: true,
            };

            let _res = execute(deps.as_mut(), env, info, msg);

            let mut env = mock_env();
            let info = mock_info(before_all.default_sender.as_str().clone(), &[]);
            let poll_state = POLL
                .load(deps.as_ref().storage, &1_u64.to_be_bytes())
                .unwrap();
            env.block.height = poll_state.end_height - 1000;

            let msg = ExecuteMsg::PresentPoll { poll_id: 1 };
            let event =
                Event::new("instantiate_contract").add_attribute("contract_address", "loterra");
            let result = ContractResult::Ok(SubMsgExecutionResponse {
                events: vec![event],
                data: None,
            });
            let _reply = reply(deps.as_mut(), env.clone(), Reply { id: 0, result }).unwrap();
            let res = execute(deps.as_mut(), env, info, msg);
            match res {
                Err(ContractError::ProposalInProgress {}) => {}
                _ => panic!("Unexpected error"),
            }
        }
        #[test]
        fn poll_expired_but_quorum_not_reached() {
            let before_all = before_all();
            let mut deps = mock_dependencies_custom(&[Coin {
                denom: "ust".to_string(),
                amount: Uint128::new(9_000_000),
            }]);
            //deps.querier.with_token_balances(Uint128::new(200_000));
            deps.querier.with_holder(
                before_all.default_sender.clone(),
                Uint128::new(99_000_000),
                Decimal::zero(),
                Decimal::zero(),
            );
            default_init(deps.as_mut());
            let _env = mock_env();
            let _info = mock_info(before_all.default_sender.as_str().clone(), &[]);
            create_poll(deps.as_mut());

            let env = mock_env();
            let info = mock_info(before_all.default_sender.as_str().clone(), &[]);
            let msg = ExecuteMsg::Vote {
                poll_id: 1,
                approve: true,
            };

            let _res = execute(deps.as_mut(), env, info, msg);

            let mut env = mock_env();
            let info = mock_info(before_all.default_sender.as_str().clone(), &[]);
            let poll_state = POLL
                .load(deps.as_ref().storage, &1_u64.to_be_bytes())
                .unwrap();
            env.block.height = poll_state.end_height + 1000;

            let msg = ExecuteMsg::PresentPoll { poll_id: 1 };
            let _res = execute(deps.as_mut(), env, info, msg).unwrap();
            let poll_state = POLL
                .load(deps.as_ref().storage, &1_u64.to_be_bytes())
                .unwrap();
            assert_eq!(poll_state.status, PollStatus::Rejected);
        }
        #[test]
        fn poll_expired_and_quorum_reached() {
            let before_all = before_all();
            let mut deps = mock_dependencies_custom(&[Coin {
                denom: "ust".to_string(),
                amount: Uint128::new(9_000_000),
            }]);
            //deps.querier.with_token_balances(Uint128::new(200_000));
            deps.querier.with_holder(
                before_all.default_sender.clone(),
                Uint128::new(100_000_000),
                Decimal::zero(),
                Decimal::zero(),
            );
            default_init(deps.as_mut());
            let env = mock_env();
            let _info = mock_info(before_all.default_sender.as_str().clone(), &[]);
            create_poll(deps.as_mut());
            let event =
                Event::new("instantiate_contract").add_attribute("contract_address", "loterra");
            let result = ContractResult::Ok(SubMsgExecutionResponse {
                events: vec![event],
                data: None,
            });
            let _reply = reply(deps.as_mut(), env.clone(), Reply { id: 0, result }).unwrap();
            let env = mock_env();
            let info = mock_info(before_all.default_sender.as_str().clone(), &[]);
            let msg = ExecuteMsg::Vote {
                poll_id: 1,
                approve: true,
            };

            let _res = execute(deps.as_mut(), env, info, msg);

            let mut env = mock_env();
            let info = mock_info(before_all.default_sender.as_str().clone(), &[]);
            let poll_state = POLL
                .load(deps.as_ref().storage, &1_u64.to_be_bytes())
                .unwrap();
            env.block.height = poll_state.end_height + 1000;

            let msg = ExecuteMsg::PresentPoll { poll_id: 1 };
            let _res = execute(deps.as_mut(), env, info, msg).unwrap();
            let poll_state = POLL
                .load(deps.as_ref().storage, &1_u64.to_be_bytes())
                .unwrap();
            assert_eq!(poll_state.status, PollStatus::Passed);
        }
        #[test]
        fn reply_lottery() {
            let before_all = before_all();
            let mut deps = mock_dependencies_custom(&[Coin {
                denom: "ust".to_string(),
                amount: Uint128::new(9_000_000),
            }]);
            deps.querier.with_holder(
                before_all.default_sender.clone(),
                Uint128::new(100_000_000),
                Decimal::zero(),
                Decimal::zero(),
            );
            default_init(deps.as_mut());
            create_poll(deps.as_mut());
            let mut env = mock_env();
            let event =
                Event::new("instantiate_contract").add_attribute("contract_address", "loterra");
            let result = ContractResult::Ok(SubMsgExecutionResponse {
                events: vec![event],
                data: None,
            });
            let _rep = reply(deps.as_mut(), env.clone(), Reply { id: 0, result }).unwrap();
            let info = mock_info(before_all.default_sender.as_str().clone(), &[]);
            let msg = ExecuteMsg::Vote {
                poll_id: 1,
                approve: true,
            };
            execute(deps.as_mut(), env.clone(), info, msg).unwrap();
            let info = mock_info(before_all.default_sender.as_str().clone(), &[]);
            let poll_state = POLL
                .load(deps.as_ref().storage, &1_u64.to_be_bytes())
                .unwrap();
            env.block.height = poll_state.end_height + 1000;

            let msg = ExecuteMsg::PresentPoll { poll_id: 1 };
            let _res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();

            let event = Event::new("message")
                .add_attribute("action", "apply poll")
                .add_attribute("applied", "true")
                .add_attribute("poll_id", 1.to_string());

            // Reply applied true
            let rep = Reply {
                id: 1,
                result: ContractResult::Ok(SubMsgExecutionResponse {
                    events: vec![event],
                    data: None,
                }),
            };
            let _res = reply(deps.as_mut(), env.clone(), rep).unwrap();
            let poll = POLL
                .load(deps.as_ref().storage, &1_u64.to_be_bytes())
                .unwrap();
            // true
            assert!(poll.applied);

            let event = Event::new("message")
                .add_attribute("action", "apply poll")
                .add_attribute("applied", "false")
                .add_attribute("poll_id", 1.to_string());
            // Reply applied false
            let rep = Reply {
                id: 1,
                result: ContractResult::Ok(SubMsgExecutionResponse {
                    events: vec![event],
                    data: None,
                }),
            };
            let _res = reply(deps.as_mut(), env, rep).unwrap();
            let poll = POLL
                .load(deps.as_ref().storage, &1_u64.to_be_bytes())
                .unwrap();
            // false
            assert!(!poll.applied);
        }
    }
}
