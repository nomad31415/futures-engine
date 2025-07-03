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
// rand = "0.8"
// dashmap = "5.5"

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{RwLock, mpsc, broadcast};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal::prelude::*;
use anyhow::Result;
use dashmap::DashMap;
use rand::Rng;

// ============================================================================
// DOMAIN MODELS
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, Hash, Eq, PartialEq)]
pub struct Symbol {
    pub base: String,
    pub quote: String,
    pub contract_type: ContractType,
}

impl std::fmt::Display for Symbol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}-{}-{:?}", self.base, self.quote, self.contract_type)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Hash, Eq, PartialEq)]
pub enum ContractType {
    Perpetual,
    Quarterly,
    Monthly,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketData {
    pub symbol: Symbol,
    pub timestamp: DateTime<Utc>,
    pub price: Decimal,
    pub volume: Decimal,
    pub open_interest: Decimal,
    pub funding_rate: Option<Decimal>,
    pub mark_price: Decimal,
    pub index_price: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub id: Uuid,
    pub user_id: Uuid,
    pub symbol: Symbol,
    pub size: Decimal, // Positive for long, negative for short
    pub entry_price: Decimal,
    pub margin: Decimal,
    pub leverage: u8,
    pub unrealized_pnl: Decimal,
    pub margin_ratio: Decimal,
    pub liquidation_price: Decimal,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub id: Uuid,
    pub user_id: Uuid,
    pub symbol: Symbol,
    pub side: OrderSide,
    pub order_type: OrderType,
    pub size: Decimal,
    pub price: Option<Decimal>,
    pub filled_size: Decimal,
    pub status: OrderStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OrderSide {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OrderType {
    Market,
    Limit,
    Stop,
    StopLimit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OrderStatus {
    Pending,
    PartiallyFilled,
    Filled,
    Cancelled,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trade {
    pub id: Uuid,
    pub order_id: Uuid,
    pub symbol: Symbol,
    pub side: OrderSide,
    pub size: Decimal,
    pub price: Decimal,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountBalance {
    pub user_id: Uuid,
    pub balance: Decimal,
    pub available_balance: Decimal,
    pub margin_used: Decimal,
    pub unrealized_pnl: Decimal,
    pub updated_at: DateTime<Utc>,
}

// ============================================================================
// LIVE DATA GENERATOR
// ============================================================================

pub struct MarketDataGenerator {
    symbols: Vec<Symbol>,
    prices: Arc<DashMap<Symbol, Decimal>>,
    broadcast_tx: broadcast::Sender<MarketData>,
}

impl MarketDataGenerator {
    pub fn new() -> (Self, broadcast::Receiver<MarketData>) {
        let symbols = vec![
            Symbol { base: "BTC".to_string(), quote: "USD".to_string(), contract_type: ContractType::Perpetual },
            Symbol { base: "ETH".to_string(), quote: "USD".to_string(), contract_type: ContractType::Perpetual },
            Symbol { base: "SOL".to_string(), quote: "USD".to_string(), contract_type: ContractType::Perpetual },
            Symbol { base: "AVAX".to_string(), quote: "USD".to_string(), contract_type: ContractType::Perpetual },
        ];

        let prices = Arc::new(DashMap::new());
        
        // Initialize with realistic prices
        prices.insert(symbols[0].clone(), Decimal::from(45000)); // BTC
        prices.insert(symbols[1].clone(), Decimal::from(2800));  // ETH
        prices.insert(symbols[2].clone(), Decimal::from(180));   // SOL
        prices.insert(symbols[3].clone(), Decimal::from(35));    // AVAX

        let (broadcast_tx, broadcast_rx) = broadcast::channel(1000);

        (Self { symbols, prices, broadcast_tx }, broadcast_rx)
    }

    pub async fn start(&self) {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(500));
        
        loop {
            interval.tick().await;
            
            for symbol in &self.symbols {
                let current_price = self.prices.get(symbol).unwrap().clone();
                
                // Generate realistic price movement (-0.5% to +0.5%)
                let mut rng = rand::thread_rng();
                let change_percent = rng.gen_range(-0.005..0.005);
                let new_price = current_price * (Decimal::ONE + Decimal::from_f64(change_percent).unwrap());
                
                self.prices.insert(symbol.clone(), new_price);
                
                let market_data = MarketData {
                    symbol: symbol.clone(),
                    timestamp: Utc::now(),
                    price: new_price,
                    volume: Decimal::from(rng.gen_range(100.0..10000.0)),
                    open_interest: Decimal::from(rng.gen_range(1000000.0..50000000.0)),
                    funding_rate: Some(Decimal::from_f64(rng.gen_range(-0.0001..0.0001)).unwrap()),
                    mark_price: new_price,
                    index_price: new_price * Decimal::from_f64(0.9999).unwrap(),
                };
                
                let _ = self.broadcast_tx.send(market_data);
            }
        }
    }

    pub fn get_current_price(&self, symbol: &Symbol) -> Option<Decimal> {
        self.prices.get(symbol).map(|p| *p)
    }
}

// ============================================================================
// MATCHING ENGINE
// ============================================================================

pub struct MatchingEngine {
    buy_orders: Arc<RwLock<HashMap<Symbol, Vec<Order>>>>,
    sell_orders: Arc<RwLock<HashMap<Symbol, Vec<Order>>>>,
    trade_tx: mpsc::Sender<Trade>,
}

impl MatchingEngine {
    pub fn new(trade_tx: mpsc::Sender<Trade>) -> Self {
        Self {
            buy_orders: Arc::new(RwLock::new(HashMap::new())),
            sell_orders: Arc::new(RwLock::new(HashMap::new())),
            trade_tx,
        }
    }

    pub async fn add_order(&self, mut order: Order) -> Result<()> {
        if order.order_type == OrderType::Market {
            // Execute market order immediately
            self.execute_market_order(&mut order).await?;
        } else {
            // Add limit order to order book
            match order.side {
                OrderSide::Buy => {
                    let mut buy_orders = self.buy_orders.write().await;
                    buy_orders.entry(order.symbol.clone()).or_insert_with(Vec::new).push(order);
                }
                OrderSide::Sell => {
                    let mut sell_orders = self.sell_orders.write().await;
                    sell_orders.entry(order.symbol.clone()).or_insert_with(Vec::new).push(order);
                }
            }
            self.try_match_orders(&order.symbol).await?;
        }
        Ok(())
    }

    async fn execute_market_order(&self, order: &mut Order) -> Result<()> {
        // Simulate market execution with some slippage
        let mut rng = rand::thread_rng();
        let slippage = Decimal::from_f64(rng.gen_range(0.9999..1.0001)).unwrap();
        let execution_price = order.price.unwrap_or(Decimal::from(45000)) * slippage;

        let trade = Trade {
            id: Uuid::new_v4(),
            order_id: order.id,
            symbol: order.symbol.clone(),
            side: order.side.clone(),
            size: order.size,
            price: execution_price,
            timestamp: Utc::now(),
        };

        order.status = OrderStatus::Filled;
        order.filled_size = order.size;
        order.updated_at = Utc::now();

        self.trade_tx.send(trade).await.map_err(|e| anyhow::anyhow!("Failed to send trade: {}", e))?;
        Ok(())
    }

    async fn try_match_orders(&self, symbol: &Symbol) -> Result<()> {
        let mut buy_orders = self.buy_orders.write().await;
        let mut sell_orders = self.sell_orders.write().await;

        let buy_book = buy_orders.entry(symbol.clone()).or_insert_with(Vec::new);
        let sell_book = sell_orders.entry(symbol.clone()).or_insert_with(Vec::new);

        // Sort orders by price (buy orders descending, sell orders ascending)
        buy_book.sort_by(|a, b| b.price.unwrap_or(Decimal::ZERO).cmp(&a.price.unwrap_or(Decimal::ZERO)));
        sell_book.sort_by(|a, b| a.price.unwrap_or(Decimal::MAX).cmp(&b.price.unwrap_or(Decimal::MAX)));

        let mut trades_to_send = Vec::new();
        let mut orders_to_remove = Vec::new();

        for (buy_idx, buy_order) in buy_book.iter_mut().enumerate() {
            for (sell_idx, sell_order) in sell_book.iter_mut().enumerate() {
                if buy_order.price.unwrap_or(Decimal::ZERO) >= sell_order.price.unwrap_or(Decimal::MAX) {
                    let trade_size = std::cmp::min(
                        buy_order.size - buy_order.filled_size,
                        sell_order.size - sell_order.filled_size
                    );

                    if trade_size > Decimal::ZERO {
                        let trade_price = sell_order.price.unwrap();

                        // Create trades for both orders
                        let buy_trade = Trade {
                            id: Uuid::new_v4(),
                            order_id: buy_order.id,
                            symbol: symbol.clone(),
                            side: OrderSide::Buy,
                            size: trade_size,
                            price: trade_price,
                            timestamp: Utc::now(),
                        };

                        let sell_trade = Trade {
                            id: Uuid::new_v4(),
                            order_id: sell_order.id,
                            symbol: symbol.clone(),
                            side: OrderSide::Sell,
                            size: trade_size,
                            price: trade_price,
                            timestamp: Utc::now(),
                        };

                        trades_to_send.push(buy_trade);
                        trades_to_send.push(sell_trade);

                        // Update order fill status
                        buy_order.filled_size += trade_size;
                        sell_order.filled_size += trade_size;

                        if buy_order.filled_size >= buy_order.size {
                            buy_order.status = OrderStatus::Filled;
                            orders_to_remove.push((true, buy_idx));
                        }

                        if sell_order.filled_size >= sell_order.size {
                            sell_order.status = OrderStatus::Filled;
                            orders_to_remove.push((false, sell_idx));
                        }
                    }
                }
            }
        }

        // Send trades
        for trade in trades_to_send {
            let _ = self.trade_tx.send(trade).await;
        }

        // Remove filled orders (in reverse order to maintain indices)
        orders_to_remove.sort_by(|a, b| b.1.cmp(&a.1));
        for (is_buy, idx) in orders_to_remove {
            if is_buy {
                buy_book.remove(idx);
            } else {
                sell_book.remove(idx);
            }
        }

        Ok(())
    }
}

// ============================================================================
// POSITION MANAGER
// ============================================================================

pub struct PositionManager {
    positions: Arc<DashMap<(Uuid, Symbol), Position>>,
    balances: Arc<DashMap<Uuid, AccountBalance>>,
}

impl PositionManager {
    pub fn new() -> Self {
        Self {
            positions: Arc::new(DashMap::new()),
            balances: Arc::new(DashMap::new()),
        }
    }

    pub fn initialize_user(&self, user_id: Uuid) {
        let balance = AccountBalance {
            user_id,
            balance: Decimal::from(100000), // Start with $100k
            available_balance: Decimal::from(100000),
            margin_used: Decimal::ZERO,
            unrealized_pnl: Decimal::ZERO,
            updated_at: Utc::now(),
        };
        self.balances.insert(user_id, balance);
    }

    pub async fn process_trade(&self, trade: &Trade, market_price: Decimal) -> Result<()> {
        let user_id = trade.order_id; // Simplified - in reality, we'd look up the user from the order
        let position_key = (user_id, trade.symbol.clone());

        let leverage = 10u8; // Default 10x leverage
        let margin_required = (trade.size * trade.price) / Decimal::from(leverage);

        if let Some(mut existing_position) = self.positions.get_mut(&position_key) {
            // Update existing position
            let new_size = if trade.side == OrderSide::Buy {
                existing_position.size + trade.size
            } else {
                existing_position.size - trade.size
            };

            if new_size.abs() > Decimal::ZERO {
                // Position still exists, update entry price (weighted average)
                let total_cost = existing_position.entry_price * existing_position.size.abs() + trade.price * trade.size;
                let total_size = existing_position.size.abs() + trade.size;
                existing_position.entry_price = total_cost / total_size;
                existing_position.size = new_size;
                existing_position.margin += margin_required;
                existing_position.updated_at = Utc::now();
                
                self.update_position_metrics(&mut existing_position, market_price);
            } else {
                // Position closed
                self.positions.remove(&position_key);
            }
        } else {
            // Create new position
            let size = if trade.side == OrderSide::Buy { trade.size } else { -trade.size };
            let mut new_position = Position {
                id: Uuid::new_v4(),
                user_id,
                symbol: trade.symbol.clone(),
                size,
                entry_price: trade.price,
                margin: margin_required,
                leverage,
                unrealized_pnl: Decimal::ZERO,
                margin_ratio: Decimal::ZERO,
                liquidation_price: Decimal::ZERO,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            };

            self.update_position_metrics(&mut new_position, market_price);
            self.positions.insert(position_key, new_position);
        }

        Ok(())
    }

    fn update_position_metrics(&self, position: &mut Position, market_price: Decimal) {
        // Calculate unrealized PnL
        position.unrealized_pnl = (market_price - position.entry_price) * position.size;
        
        // Calculate margin ratio
        let position_value = market_price * position.size.abs();
        position.margin_ratio = (position.margin + position.unrealized_pnl) / position_value;
        
        // Calculate liquidation price (simplified)
        let liquidation_threshold = Decimal::from_f64(0.05).unwrap(); // 5% margin ratio
        if position.size > Decimal::ZERO {
            // Long position
            position.liquidation_price = position.entry_price * (Decimal::ONE - liquidation_threshold);
        } else {
            // Short position
            position.liquidation_price = position.entry_price * (Decimal::ONE + liquidation_threshold);
        }
    }

    pub async fn update_all_positions(&self, market_data: &MarketData) {
        for mut position in self.positions.iter_mut() {
            if position.symbol == market_data.symbol {
                self.update_position_metrics(&mut position, market_data.price);
            }
        }
    }

    pub fn get_all_positions(&self) -> Vec<Position> {
        self.positions.iter().map(|entry| entry.value().clone()).collect()
    }

    pub fn check_liquidations(&self, market_data: &MarketData) -> Vec<Position> {
        let mut liquidations = Vec::new();
        
        for position in self.positions.iter() {
            if position.symbol == market_data.symbol {
                let should_liquidate = if position.size > Decimal::ZERO {
                    // Long position: liquidate if price drops below liquidation price
                    market_data.price <= position.liquidation_price
                } else {
                    // Short position: liquidate if price rises above liquidation price
                    market_data.price >= position.liquidation_price
                };

                if should_liquidate {
                    liquidations.push(position.clone());
                }
            }
        }
        
        liquidations
    }
}

// ============================================================================
// TRADING SYSTEM
// ============================================================================

pub struct TradingSystem {
    market_data_generator: Arc<MarketDataGenerator>,
    matching_engine: Arc<MatchingEngine>,
    position_manager: Arc<PositionManager>,
    orders: Arc<DashMap<Uuid, Order>>,
    trades: Arc<RwLock<Vec<Trade>>>,
    market_data_tx: broadcast::Sender<MarketData>,
}

impl TradingSystem {
    pub fn new() -> (Self, broadcast::Receiver<MarketData>) {
        let (trade_tx, mut trade_rx) = mpsc::channel::<Trade>(1000);
        let (market_data_generator, market_data_rx) = MarketDataGenerator::new();
        let matching_engine = Arc::new(MatchingEngine::new(trade_tx));
        let position_manager = Arc::new(PositionManager::new());
        let orders = Arc::new(DashMap::new());
        let trades = Arc::new(RwLock::new(Vec::new()));
        
        let (market_data_tx, _) = broadcast::channel(1000);

        let system = Self {
            market_data_generator: Arc::new(market_data_generator),
            matching_engine,
            position_manager: position_manager.clone(),
            orders: orders.clone(),
            trades: trades.clone(),
            market_data_tx: market_data_tx.clone(),
        };

        // Initialize test user
        system.position_manager.initialize_user(Uuid::new_v4());

        // Start background tasks
        let system_clone = system.clone();
        tokio::spawn(async move {
            system_clone.start_background_tasks(trade_rx, market_data_rx).await;
        });

        (system, market_data_rx)
    }

    async fn start_background_tasks(&self, mut trade_rx: mpsc::Receiver<Trade>, mut market_data_rx: broadcast::Receiver<MarketData>) {
        let market_gen = self.market_data_generator.clone();
        let pos_mgr = self.position_manager.clone();
        let trades = self.trades.clone();
        let market_tx = self.market_data_tx.clone();

        // Start market data generator
        tokio::spawn(async move {
            market_gen.start().await;
        });

        // Process trades and market data
        loop {
            tokio::select! {
                Some(trade) = trade_rx.recv() => {
                    println!("🔄 Trade executed: {} {} {} @ {}", 
                        trade.symbol, 
                        match trade.side { OrderSide::Buy => "BUY", OrderSide::Sell => "SELL" },
                        trade.size, 
                        trade.price
                    );
                    
                    // Get current market price for position calculation
                    if let Some(market_price) = self.market_data_generator.get_current_price(&trade.symbol) {
                        let _ = pos_mgr.process_trade(&trade, market_price).await;
                    }
                    
                    // Store trade
                    trades.write().await.push(trade);
                }
                
                Ok(market_data) = market_data_rx.recv() => {
                    // Update positions with new market data
                    pos_mgr.update_all_positions(&market_data).await;
                    
                    // Check for liquidations
                    let liquidations = pos_mgr.check_liquidations(&market_data);
                    for liquidation in liquidations {
                        println!("⚠️  LIQUIDATION: Position {} for user {} in {}", 
                            liquidation.id, liquidation.user_id, liquidation.symbol);
                    }
                    
                    // Broadcast market data to dashboard
                    let _ = market_tx.send(market_data);
                }
            }
        }
    }

    pub async fn place_order(&self, mut order: Order) -> Result<Uuid> {
        // Set current market price for market orders
        if order.order_type == OrderType::Market {
            order.price = self.market_data_generator.get_current_price(&order.symbol);
        }

        let order_id = order.id;
        println!("📝 Order placed: {} {} {} {} @ {:?}", 
            order.symbol,
            match order.side { OrderSide::Buy => "BUY", OrderSide::Sell => "SELL" },
            order.size,
            match order.order_type { OrderType::Market => "MARKET", OrderType::Limit => "LIMIT", _ => "OTHER" },
            order.price
        );

        self.orders.insert(order_id, order.clone());
        self.matching_engine.add_order(order).await?;
        Ok(order_id)
    }

    pub fn get_current_prices(&self) -> HashMap<Symbol, Decimal> {
        let mut prices = HashMap::new();
        for symbol in &[
            Symbol { base: "BTC".to_string(), quote: "USD".to_string(), contract_type: ContractType::Perpetual },
            Symbol { base: "ETH".to_string(), quote: "USD".to_string(), contract_type: ContractType::Perpetual },
            Symbol { base: "SOL".to_string(), quote: "USD".to_string(), contract_type: ContractType::Perpetual },
            Symbol { base: "AVAX".to_string(), quote: "USD".to_string(), contract_type: ContractType::Perpetual },
        ] {
            if let Some(price) = self.market_data_generator.get_current_price(symbol) {
                prices.insert(symbol.clone(), price);
            }
        }
        prices
    }

    pub fn get_positions(&self) -> Vec<Position> {
        self.position_manager.get_all_positions()
    }

    pub async fn get_recent_trades(&self, limit: usize) -> Vec<Trade> {
        let trades = self.trades.read().await;
        trades.iter().rev().take(limit).cloned().collect()
    }

    pub fn get_orders(&self) -> Vec<Order> {
        self.orders.iter().map(|entry| entry.value().clone()).collect()
    }
}

impl Clone for TradingSystem {
    fn clone(&self) -> Self {
        Self {
            market_data_generator: self.market_data_generator.clone(),
            matching_engine: self.matching_engine.clone(),
            position_manager: self.position_manager.clone(),
            orders: self.orders.clone(),
            trades: self.trades.clone(),
            market_data_tx: self.market_data_tx.clone(),
        }
    }
}

// ============================================================================
// API ENDPOINTS
// ============================================================================

use axum::{
    extract::{Path, Query, State, WebSocketUpgrade, ws::WebSocket},
    http::StatusCode,
    response::{Html, Json, IntoResponse},
    routing::{get, post},
    Router,
};
use tower_http::{cors::CorsLayer, services::ServeDir};
use std::collections::HashMap as StdHashMap;

#[derive(Clone)]
pub struct AppState {
    trading_system: TradingSystem,
}

#[derive(Deserialize)]
pub struct PlaceOrderRequest {
    pub symbol: String,
    pub side: String,
    pub order_type: String,
    pub size: String,
    pub price: Option<String>,
}

pub fn create_router(state: AppState) -> Router {
    Router::new()
        // API routes
        .route("/api/orders", post(place_order))
        .route("/api/orders", get(get_orders))
        .route("/api/positions", get(get_positions))
        .route("/api/prices", get(get_prices))
        .route("/api/trades", get(get_trades))
        .route("/api/ws", get(websocket_handler))
        // Dashboard route
        .route("/", get(dashboard))
        .route("/dashboard", get(dashboard))
        .with_state(state)
        .layer(CorsLayer::permissive())
}

async fn place_order(
    State(state): State<AppState>,
    Json(req): Json<PlaceOrderRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let symbol = Symbol {
        base: req.symbol.split('-').nth(0).unwrap_or("BTC").to_string(),
        quote: "USD".to_string(),
        contract_type: ContractType::Perpetual,
    };

    let side = match req.side.as_str() {
        "buy" => OrderSide::Buy,
        "sell" => OrderSide::Sell,
        _ => return Err(StatusCode::BAD_REQUEST),
    };

    let order_type = match req.order_type.as_str() {
        "market" => OrderType::Market,
        "limit" => OrderType::Limit,
        _ => return Err(StatusCode::BAD_REQUEST),
    };

    let size = req.size.parse::<f64>().map_err(|_| StatusCode::BAD_REQUEST)?;
    let price = req.price.map(|p| p.parse::<f64>().map_err(|_| StatusCode::BAD_REQUEST)).transpose()?;

    let order = Order {
        id: Uuid::new_v4(),
        user_id: Uuid::new_v4(), // Simplified - in reality, get from auth
        symbol,
        side,
        order_type,
        size: Decimal::from_f64(size).unwrap(),
        price: price.map(|p| Decimal::from_f64(p).unwrap()),
        filled_size: Decimal::ZERO,
        status: OrderStatus::Pending,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    match state.trading_system.place_order(order).await {
        Ok(order_id) => Ok(Json(serde_json::json!({
            "success": true,
            "order_id": order_id,
            "message": "Order placed successfully"
        }))),
        Err(e) => {
            eprintln!("Error placing order: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn get_orders(State(state): State<AppState>) -> Json<Vec<Order>> {
    Json(state.trading_system.get_orders())
}

async fn get_positions(State(state): State<AppState>) -> Json<Vec<Position>> {
    Json(state.trading_system.get_positions())
}

async fn get_prices(State(state): State<AppState>) -> Json<StdHashMap<String, f64>> {
    let prices = state.trading_system.get_current_prices();
    let mut result = StdHashMap::new();
    
    for (symbol, price) in prices {
        result.insert(symbol.to_string(), price.to_f64().unwrap_or(0.0));
    }
    
    Json(result)
}

#[derive(Deserialize)]
struct TradesQuery {
    limit: Option<usize>,
}

async fn get_trades(
    State(state): State<AppState>,
    Query(params): Query<TradesQuery>,
) -> Json<Vec<Trade>> {
    let limit = params.limit.unwrap_or(50);
    Json(state.trading_system.get_recent_trades
