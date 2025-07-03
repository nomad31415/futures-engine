// Cargo.toml
// [package]
// name = "crypto-futures-system"
// version = "0.1.0"
// edition = "2021"
//
// [dependencies]
// tokio = { version = "1.0", features = ["full"] }
// serde = { version = "1.0", features = ["derive"] }
// serde_json = "1.0"
// uuid = { version = "1.0", features = ["v4", "serde"] }
// chrono = { version = "0.4", features = ["serde"] }
// rust_decimal = { version = "1.0", features = ["serde"] }
// axum = "0.7"
// tower = "0.4"
// tower-http = { version = "0.5", features = ["cors", "fs"] }
// tracing = "0.1"
// tracing-subscriber = "0.3"
// anyhow = "1.0"
// thiserror = "1.0"
// rand = "0.8"
// dashmap = "5.5"
// reqwest = { version = "0.11", features = ["json"] }
// hmac = "0.12"
// sha2 = "0.10"
// hex = "0.4"
// base64 = "0.21"

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{RwLock, mpsc, broadcast};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc, Duration};
use rust_decimal::Decimal;
use rust_decimal::prelude::*;
use anyhow::Result;
use dashmap::DashMap;
use rand::Rng;

// ============================================================================
// ADVANCED FUTURES DOMAIN MODELS
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, Hash, Eq, PartialEq)]
pub struct FuturesContract {
    pub symbol: String,               // BTCUSDT, ETHUSDT
    pub base_asset: String,          // BTC, ETH
    pub quote_asset: String,         // USDT, USD
    pub contract_type: ContractType,
    pub contract_size: Decimal,      // Contract multiplier
    pub tick_size: Decimal,          // Minimum price increment
    pub min_qty: Decimal,            // Minimum order quantity
    pub max_qty: Decimal,            // Maximum order quantity
    pub margin_asset: String,        // USDT, USD
    pub settlement_asset: String,    // USDT for USDT-M, BTC for COIN-M
    pub listing_date: DateTime<Utc>,
    pub expiry_date: Option<DateTime<Utc>>, // None for perpetual
    pub delivery_date: Option<DateTime<Utc>>,
    pub status: ContractStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, Hash, Eq, PartialEq)]
pub enum ContractType {
    Perpetual,              // No expiry
    CurrentQuarter,         // 3 months
    NextQuarter,           // 6 months
    CurrentMonth,          // 1 month
    NextMonth,             // 2 months
    BiQuarterly,           // 6 months
}

#[derive(Debug, Clone, Serialize, Deserialize, Hash, Eq, PartialEq)]
pub enum ContractStatus {
    PendingTrading,
    Trading,
    PreDelivery,
    Delivering,
    Delivered,
    PreSettle,
    Settling,
    Close,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuturesMarketData {
    pub symbol: String,
    pub timestamp: DateTime<Utc>,
    pub mark_price: Decimal,         // Fair price for liquidation
    pub index_price: Decimal,        // Spot index price
    pub last_price: Decimal,         // Last traded price
    pub bid_price: Decimal,
    pub ask_price: Decimal,
    pub open_price_24h: Decimal,
    pub high_price_24h: Decimal,
    pub low_price_24h: Decimal,
    pub volume_24h: Decimal,
    pub quote_volume_24h: Decimal,
    pub open_interest: Decimal,      // Total open contracts
    pub funding_rate: Decimal,       // Current funding rate
    pub next_funding_time: DateTime<Utc>,
    pub price_change_24h: Decimal,
    pub price_change_percent_24h: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuturesPosition {
    pub symbol: String,
    pub position_amt: Decimal,       // Positive for long, negative for short
    pub entry_price: Decimal,
    pub mark_price: Decimal,
    pub unrealized_pnl: Decimal,
    pub margin_type: MarginType,
    pub isolated_margin: Decimal,
    pub position_side: PositionSide,
    pub leverage: u8,
    pub max_notional_value: Decimal,
    pub liquidation_price: Decimal,
    pub bankruptcy_price: Decimal,
    pub adl_quantile: u8,            // Auto-deleveraging queue position
    pub update_time: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MarginType {
    Isolated,
    Cross,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PositionSide {
    Both,      // One-way mode
    Long,      // Hedge mode long
    Short,     // Hedge mode short
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuturesOrder {
    pub order_id: String,
    pub client_order_id: String,
    pub symbol: String,
    pub side: OrderSide,
    pub position_side: PositionSide,
    pub order_type: FuturesOrderType,
    pub time_in_force: TimeInForce,
    pub quantity: Decimal,
    pub price: Option<Decimal>,
    pub stop_price: Option<Decimal>,
    pub close_position: bool,        // Close all position
    pub activation_price: Option<Decimal>, // For trailing stops
    pub callback_rate: Option<Decimal>,    // For trailing stops
    pub working_type: WorkingType,
    pub price_protect: bool,         // Reduce only order protection
    pub reduce_only: bool,
    pub status: FuturesOrderStatus,
    pub filled_qty: Decimal,
    pub avg_price: Decimal,
    pub commission: Decimal,
    pub commission_asset: String,
    pub created_time: DateTime<Utc>,
    pub updated_time: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FuturesOrderType {
    Market,
    Limit,
    Stop,                    // Stop-loss market
    StopMarket,             // Stop-loss market
    TakeProfit,             // Take-profit market
    TakeProfitMarket,       // Take-profit market
    StopLimit,              // Stop-loss limit
    TakeProfitLimit,        // Take-profit limit
    TrailingStopMarket,     // Trailing stop market
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TimeInForce {
    GTC,    // Good Till Cancel
    IOC,    // Immediate or Cancel
    FOK,    // Fill or Kill
    GTX,    // Good Till Crossing (Post-only)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WorkingType {
    MarkPrice,
    ContractPrice,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FuturesOrderStatus {
    New,
    PartiallyFilled,
    Filled,
    Canceled,
    Rejected,
    Expired,
    NewInsurance,          // Liquidation with insurance fund
    NewADL,               // Auto-deleveraging liquidation
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OrderSide {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FundingRate {
    pub symbol: String,
    pub funding_rate: Decimal,
    pub funding_time: DateTime<Utc>,
    pub mark_price: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeverageBracket {
    pub bracket: u8,
    pub initial_leverage: u8,
    pub notional_cap: Decimal,
    pub notional_floor: Decimal,
    pub maint_margin_ratio: Decimal,
    pub cum: Decimal,                // Auxiliary number for quick calculation
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountInfo {
    pub fee_tier: u8,
    pub can_trade: bool,
    pub can_deposit: bool,
    pub can_withdraw: bool,
    pub update_time: DateTime<Utc>,
    pub multi_assets_margin: bool,
    pub total_initial_margin: Decimal,
    pub total_maint_margin: Decimal,
    pub total_margin_balance: Decimal,
    pub total_cross_wallet_balance: Decimal,
    pub total_cross_un_pnl: Decimal,
    pub available_balance: Decimal,
    pub max_withdraw_amount: Decimal,
}

// ============================================================================
// FUTURES ENGINE
// ============================================================================

pub struct FuturesEngine {
    contracts: Arc<DashMap<String, FuturesContract>>,
    positions: Arc<DashMap<String, FuturesPosition>>,
    orders: Arc<DashMap<String, FuturesOrder>>,
    market_data: Arc<DashMap<String, FuturesMarketData>>,
    funding_rates: Arc<DashMap<String, FundingRate>>,
    leverage_brackets: Arc<DashMap<String, Vec<LeverageBracket>>>,
    trade_tx: broadcast::Sender<FuturesTrade>,
    liquidation_tx: broadcast::Sender<LiquidationEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuturesTrade {
    pub id: String,
    pub order_id: String,
    pub symbol: String,
    pub side: OrderSide,
    pub quantity: Decimal,
    pub price: Decimal,
    pub quote_qty: Decimal,
    pub commission: Decimal,
    pub commission_asset: String,
    pub is_maker: bool,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiquidationEvent {
    pub symbol: String,
    pub side: OrderSide,
    pub order_type: String,
    pub time_in_force: String,
    pub quantity: Decimal,
    pub price: Decimal,
    pub avg_price: Decimal,
    pub order_status: String,
    pub last_filled_qty: Decimal,
    pub filled_accumulated_qty: Decimal,
    pub trade_time: DateTime<Utc>,
}

impl FuturesEngine {
    pub fn new() -> Self {
        let (trade_tx, _) = broadcast::channel(1000);
        let (liquidation_tx, _) = broadcast::channel(1000);

        let engine = Self {
            contracts: Arc::new(DashMap::new()),
            positions: Arc::new(DashMap::new()),
            orders: Arc::new(DashMap::new()),
            market_data: Arc::new(DashMap::new()),
            funding_rates: Arc::new(DashMap::new()),
            leverage_brackets: Arc::new(DashMap::new()),
            trade_tx,
            liquidation_tx,
        };

        engine.initialize_contracts();
        engine.initialize_leverage_brackets();
        engine
    }

    fn initialize_contracts(&self) {
        let contracts = vec![
            ("BTCUSDT", "BTC", "USDT", Decimal::from(100000), Decimal::from_f64(0.1).unwrap()),
            ("ETHUSDT", "ETH", "USDT", Decimal::from(50000), Decimal::from_f64(0.01).unwrap()),
            ("SOLUSDT", "SOL", "USDT", Decimal::from(10000), Decimal::from_f64(0.001).unwrap()),
            ("ADAUSDT", "ADA", "USDT", Decimal::from(100000), Decimal::from_f64(0.0001).unwrap()),
        ];

        for (symbol, base, quote, max_qty, tick_size) in contracts {
            let contract = FuturesContract {
                symbol: symbol.to_string(),
                base_asset: base.to_string(),
                quote_asset: quote.to_string(),
                contract_type: ContractType::Perpetual,
                contract_size: Decimal::ONE,
                tick_size,
                min_qty: Decimal::from_f64(0.001).unwrap(),
                max_qty,
                margin_asset: quote.to_string(),
                settlement_asset: quote.to_string(),
                listing_date: Utc::now() - Duration::days(365),
                expiry_date: None,
                delivery_date: None,
                status: ContractStatus::Trading,
            };
            self.contracts.insert(symbol.to_string(), contract);
        }
    }

    fn initialize_leverage_brackets(&self) {
        let brackets = vec![
            LeverageBracket { bracket: 1, initial_leverage: 125, notional_cap: Decimal::from(5000), notional_floor: Decimal::ZERO, maint_margin_ratio: Decimal::from_f64(0.004).unwrap(), cum: Decimal::ZERO },
            LeverageBracket { bracket: 2, initial_leverage: 100, notional_cap: Decimal::from(25000), notional_floor: Decimal::from(5000), maint_margin_ratio: Decimal::from_f64(0.005).unwrap(), cum: Decimal::from(5) },
            LeverageBracket { bracket: 3, initial_leverage: 50, notional_cap: Decimal::from(100000), notional_floor: Decimal::from(25000), maint_margin_ratio: Decimal::from_f64(0.01).unwrap(), cum: Decimal::from(130) },
            LeverageBracket { bracket: 4, initial_leverage: 20, notional_cap: Decimal::from(250000), notional_floor: Decimal::from(100000), maint_margin_ratio: Decimal::from_f64(0.025).unwrap(), cum: Decimal::from(1630) },
            LeverageBracket { bracket: 5, initial_leverage: 10, notional_cap: Decimal::from(1000000), notional_floor: Decimal::from(250000), maint_margin_ratio: Decimal::from_f64(0.05).unwrap(), cum: Decimal::from(7880) },
        ];

        for contract in self.contracts.iter() {
            self.leverage_brackets.insert(contract.key().clone(), brackets.clone());
        }
    }

    pub async fn place_order(&self, order: FuturesOrder) -> Result<String> {
        // Validate order
        self.validate_order(&order).await?;

        // Calculate required margin
        let margin_required = self.calculate_required_margin(&order).await?;
        
        // Risk checks
        self.perform_risk_checks(&order, margin_required).await?;

        // Store order
        let order_id = order.order_id.clone();
        self.orders.insert(order_id.clone(), order.clone());

        // Process order based on type
        match order.order_type {
            FuturesOrderType::Market => {
                self.execute_market_order(order).await?;
            }
            _ => {
                // Add to order book for matching
                println!("📝 Limit order added to book: {} {} {} @ {:?}", 
                    order.symbol, 
                    match order.side { OrderSide::Buy => "BUY", OrderSide::Sell => "SELL" },
                    order.quantity,
                    order.price
                );
            }
        }

        Ok(order_id)
    }

    async fn validate_order(&self, order: &FuturesOrder) -> Result<()> {
        // Check if contract exists
        let contract = self.contracts.get(&order.symbol)
            .ok_or_else(|| anyhow::anyhow!("Contract not found: {}", order.symbol))?;

        // Check quantity limits
        if order.quantity < contract.min_qty || order.quantity > contract.max_qty {
            return Err(anyhow::anyhow!("Invalid quantity: {} (min: {}, max: {})", 
                order.quantity, contract.min_qty, contract.max_qty));
        }

        // Check tick size for limit orders
        if let Some(price) = order.price {
            let remainder = price % contract.tick_size;
            if remainder != Decimal::ZERO {
                return Err(anyhow::anyhow!("Invalid tick size: {} (tick: {})", 
                    price, contract.tick_size));
            }
        }

        Ok(())
    }

    async fn calculate_required_margin(&self, order: &FuturesOrder) -> Result<Decimal> {
        let market_data = self.market_data.get(&order.symbol)
            .ok_or_else(|| anyhow::anyhow!("No market data for {}", order.symbol))?;

        let notional = order.quantity * market_data.mark_price;
        let leverage = self.get_max_leverage(&order.symbol, notional).await?;
        
        Ok(notional / Decimal::from(leverage))
    }

    async fn get_max_leverage(&self, symbol: &str, notional: Decimal) -> Result<u8> {
        let brackets = self.leverage_brackets.get(symbol)
            .ok_or_else(|| anyhow::anyhow!("No leverage brackets for {}", symbol))?;

        for bracket in brackets.iter() {
            if notional >= bracket.notional_floor && notional < bracket.notional_cap {
                return Ok(bracket.initial_leverage);
            }
        }

        Ok(1) // Minimum leverage
    }

    async fn perform_risk_checks(&self, _order: &FuturesOrder, _margin_required: Decimal) -> Result<()> {
        // Implement comprehensive risk checks:
        // - Available balance check
        // - Position limits
        // - Maximum leverage limits
        // - ADL queue position
        // - Market impact assessment
        
        // For now, simplified check
        Ok(())
    }

    async fn execute_market_order(&self, mut order: FuturesOrder) -> Result<()> {
        let market_data = self.market_data.get(&order.symbol)
            .ok_or_else(|| anyhow::anyhow!("No market data for {}", order.symbol))?;

        // Simulate execution with slippage
        let mut rng = rand::thread_rng();
        let slippage_factor = Decimal::from_f64(rng.gen_range(0.9998..1.0002)).unwrap();
        let execution_price = match order.side {
            OrderSide::Buy => market_data.ask_price * slippage_factor,
            OrderSide::Sell => market_data.bid_price * slippage_factor,
        };

        // Update order
        order.status = FuturesOrderStatus::Filled;
        order.filled_qty = order.quantity;
        order.avg_price = execution_price;
        order.updated_time = Utc::now();

        // Calculate commission (0.04% taker fee)
        let commission = order.quantity * execution_price * Decimal::from_f64(0.0004).unwrap();
        order.commission = commission;

        // Create trade
        let trade = FuturesTrade {
            id: Uuid::new_v4().to_string(),
            order_id: order.order_id.clone(),
            symbol: order.symbol.clone(),
            side: order.side.clone(),
            quantity: order.quantity,
            price: execution_price,
            quote_qty: order.quantity * execution_price,
            commission,
            commission_asset: "USDT".to_string(),
            is_maker: false,
            timestamp: Utc::now(),
        };

        // Update position
        self.update_position(&trade).await?;

        // Store updated order
        self.orders.insert(order.order_id.clone(), order);

        // Broadcast trade
        let _ = self.trade_tx.send(trade.clone());

        println!("✅ Market order executed: {} {} {} @ {}", 
            trade.symbol, 
            match trade.side { OrderSide::Buy => "BUY", OrderSide::Sell => "SELL" },
            trade.quantity, 
            trade.price
        );

        Ok(())
    }

    async fn update_position(&self, trade: &FuturesTrade) -> Result<()> {
        let position_key = format!("{}_{}", "user123", trade.symbol); // Simplified user ID
        
        if let Some(mut position) = self.positions.get_mut(&position_key) {
            // Update existing position
            let old_position_amt = position.position_amt;
            let trade_amt = match trade.side {
                OrderSide::Buy => trade.quantity,
                OrderSide::Sell => -trade.quantity,
            };

            if old_position_amt.is_sign_positive() == trade_amt.is_sign_positive() || old_position_amt.is_zero() {
                // Same direction or new position - update entry price
                let total_cost = position.entry_price * position.position_amt.abs() + trade.price * trade.quantity;
                let total_qty = position.position_amt.abs() + trade.quantity;
                position.entry_price = total_cost / total_qty;
                position.position_amt += trade_amt;
            } else {
                // Opposite direction - reduce or flip position
                position.position_amt += trade_amt;
                if position.position_amt.is_zero() {
                    position.entry_price = Decimal::ZERO;
                }
            }

            position.update_time = Utc::now();
        } else {
            // Create new position
            let position_amt = match trade.side {
                OrderSide::Buy => trade.quantity,
                OrderSide::Sell => -trade.quantity,
            };

            let position = FuturesPosition {
                symbol: trade.symbol.clone(),
                position_amt,
                entry_price: trade.price,
                mark_price: trade.price,
                unrealized_pnl: Decimal::ZERO,
                margin_type: MarginType::Cross,
                isolated_margin: Decimal::ZERO,
                position_side: PositionSide::Both,
                leverage: 10,
                max_notional_value: Decimal::from(1000000),
                liquidation_price: self.calculate_liquidation_price(&trade.symbol, position_amt, trade.price, 10).await?,
                bankruptcy_price: Decimal::ZERO,
                adl_quantile: 1,
                update_time: Utc::now(),
            };

            self.positions.insert(position_key, position);
        }

        Ok(())
    }

    async fn calculate_liquidation_price(&self, symbol: &str, position_amt: Decimal, entry_price: Decimal, leverage: u8) -> Result<Decimal> {
        let brackets = self.leverage_brackets.get(symbol)
            .ok_or_else(|| anyhow::anyhow!("No leverage brackets for {}", symbol))?;

        let notional = position_amt.abs() * entry_price;
        let mut maint_margin_ratio = Decimal::from_f64(0.025).unwrap(); // Default 2.5%

        // Find appropriate maintenance margin ratio
        for bracket in brackets.iter() {
            if notional >= bracket.notional_floor && notional < bracket.notional_cap {
                maint_margin_ratio = bracket.maint_margin_ratio;
                break;
            }
        }

        // Calculate liquidation price
        let initial_margin_ratio = Decimal::ONE / Decimal::from(leverage);
        
        if position_amt > Decimal::ZERO {
            // Long position
            Ok(entry_price * (Decimal::ONE - initial_margin_ratio + maint_margin_ratio))
        } else {
            // Short position  
            Ok(entry_price * (Decimal::ONE + initial_margin_ratio - maint_margin_ratio))
        }
    }

    pub async fn update_market_data(&self, symbol: &str) {
        let mut rng = rand::thread_rng();
        
        // Get current price or set default
        let current_price = self.market_data.get(symbol)
            .map(|md| md.last_price)
            .unwrap_or_else(|| match symbol {
                "BTCUSDT" => Decimal::from(45000),
                "ETHUSDT" => Decimal::from(2800),
                "SOLUSDT" => Decimal::from(180),
                "ADAUSDT" => Decimal::from_f64(0.45).unwrap(),
                _ => Decimal::from(100),
            });

        // Generate realistic price movement
        let change_percent = rng.gen_range(-0.002..0.002); // -0.2% to +0.2%
        let new_price = current_price * (Decimal::ONE + Decimal::from_f64(change_percent).unwrap());
        
        let spread_bps = rng.gen_range(1..10); // 1-10 basis points spread
        let spread = new_price * Decimal::from(spread_bps) / Decimal::from(10000);
        
        let market_data = FuturesMarketData {
            symbol: symbol.to_string(),
            timestamp: Utc::now(),
            mark_price: new_price,
            index_price: new_price * Decimal::from_f64(0.9999).unwrap(),
            last_price: new_price,
            bid_price: new_price - spread / Decimal::from(2),
            ask_price: new_price + spread / Decimal::from(2),
            open_price_24h: current_price,
            high_price_24h: new_price * Decimal::from_f64(1.02).unwrap(),
            low_price_24h: new_price * Decimal::from_f64(0.98).unwrap(),
            volume_24h: Decimal::from(rng.gen_range(1000000.0..50000000.0)),
            quote_volume_24h: Decimal::from(rng.gen_range(10000000000.0..500000000000.0)),
            open_interest: Decimal::from(rng.gen_range(100000.0..10000000.0)),
            funding_rate: Decimal::from_f64(rng.gen_range(-0.001..0.001)).unwrap(),
            next_funding_time: Utc::now() + Duration::hours(8),
            price_change_24h: new_price - current_price,
            price_change_percent_24h: ((new_price - current_price) / current_price) * Decimal::from(100),
        };

        self.market_data.insert(symbol.to_string(), market_data.clone());
        
        // Update positions with new mark price
        self.update_positions_pnl(symbol, new_price).await;
        
        // Check for liquidations
        self.check_liquidations(symbol, new_price).await;
    }

    async fn update_positions_pnl(&self, symbol: &str, mark_price: Decimal) {
        for mut position in self.positions.iter_mut() {
            if position.symbol == symbol {
                position.mark_price = mark_price;
                position.unrealized_pnl = (mark_price - position.entry_price) * position.position_amt;
                position.update_time = Utc::now();
            }
        }
    }

    async fn check_liquidations(&self, symbol: &str, mark_price: Decimal) {
        let mut liquidations = Vec::new();

        for position in self.positions.iter() {
            if position.symbol == symbol {
                let should_liquidate = if position.position_amt > Decimal::ZERO {
                    // Long position
                    mark_price <= position.liquidation_price
                } else if position.position_amt < Decimal::ZERO {
                    // Short position
                    mark_price >= position.liquidation_price
                } else {
                    false
                };

                if should_liquidate {
                    liquidations.push(position.clone());
                }
            }
        }

        for position in liquidations {
            self.liquidate_position(position).await;
        }
    }

    async fn liquidate_position(&self, position: FuturesPosition) {
        let liquidation_event = LiquidationEvent {
            symbol: position.symbol.clone(),
            side: if position.position_amt > Decimal::ZERO { OrderSide::Sell } else { OrderSide::Buy },
            order_type: "MARKET".to_string(),
            time_in_force: "IOC".to_string(),
            quantity: position.position_amt.abs(),
            price: position.liquidation_price,
            avg_price: position.liquidation_price,
            order_status: "FILLED".to_string(),
            last_filled_qty: position.position_amt.abs(),
            filled_accumulated_qty: position.position_amt.abs(),
            trade_time: Utc::now(),
        };

        println!("🚨 LIQUIDATION: {} position {} {} @ {}", 
            position.symbol,
            if position.position_amt > Decimal::ZERO { "LONG" } else { "SHORT" },
            position.position_amt.abs(),
            position.liquidation_price
        );

        // Remove position
        let position_key = format!("user123_{}", position.symbol);
        self.positions.remove(&position_key);

        // Broadcast liquidation event
        let _ = self.liquidation_tx.send(liquidation_event);
    }

    pub fn get_all_positions(&self) -> Vec<FuturesPosition> {
        self.positions.iter().map(|entry| entry.value().clone()).collect()
    }

    pub fn get_market_data(&self, symbol: &str) -> Option<FuturesMarketData> {
        self.market_data.get(symbol).map(|md| md.clone())
    }

    pub fn get_all_market_data(&self) -> Vec<FuturesMarketData> {
        self.market_data.iter().map(|entry| entry.value().clone()).collect()
    }

    pub fn get_orders(&self) -> Vec<FuturesOrder> {
        self.orders.iter().map(|entry| entry.value().clone()).collect()
    }
}

// ============================================================================
// EXCHANGE API COMPATIBILITY LAYER
// ============================================================================

// Binance Futures API compatibility
pub mod binance_
