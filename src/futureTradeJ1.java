// FuturesContract.java - Base contract definition
public abstract class FuturesContract {
    private final String contractId;
    private final String symbol;          // e.g., BTC-USD-PERP or ETH-USD-QUARTERLY
    private final ContractType type;      // PERPETUAL, QUARTERLY, MONTHLY
    private final Currency baseCurrency;  // BTC, ETH, etc.
    private final Currency quoteCurrency; // USD, USDT, etc.
    private final BigDecimal contractSize; // 1 BTC, 0.1 ETH, etc.
    private final BigDecimal tickSize;    // Minimum price increment
    private final BigDecimal minOrderSize;
    private final BigDecimal maxOrderSize;
    private final LocalDateTime listingDate;
    private final LocalDateTime expiryDate; // Null for perpetuals
    private final FundingRateConfig fundingRateConfig; // For perpetuals
    
    // Contract specifications methods...
    public abstract BigDecimal calculateMargin(BigDecimal price, BigDecimal quantity, PositionSide side);
    public abstract BigDecimal calculatePnl(OpenPosition position, BigDecimal markPrice);
}

// PerpetualContract.java
public class PerpetualContract extends FuturesContract {
    private FundingRateConfig fundingRateConfig;
    
    @Override
    public BigDecimal calculateMargin(BigDecimal price, BigDecimal quantity, PositionSide side) {
        BigDecimal notional = price.multiply(quantity).multiply(getContractSize());
        return notional.multiply(getInitialMarginRequirement());
    }
    
    @Override
    public BigDecimal calculatePnl(OpenPosition position, BigDecimal markPrice) {
        BigDecimal priceDiff = position.getSide() == PositionSide.LONG 
            ? markPrice.subtract(position.getEntryPrice())
            : position.getEntryPrice().subtract(markPrice);
        return priceDiff.multiply(position.getQuantity()).multiply(getContractSize());
    }
    
    public BigDecimal calculateFundingPayment(BigDecimal positionSize, BigDecimal fundingRate) {
        return positionSize.multiply(fundingRate);
    }
}

// QuarterlyContract.java
public class QuarterlyContract extends FuturesContract {
    private final LocalDateTime expiryDate;
    private final LocalDateTime deliveryDate;
    
    @Override
    public BigDecimal calculateMargin(BigDecimal price, BigDecimal quantity, PositionSide side) {
        // Different margin calculation for dated contracts
        BigDecimal notional = price.multiply(quantity).multiply(getContractSize());
        return notional.multiply(getInitialMarginRequirement())
                      .multiply(BigDecimal.valueOf(1.1)); // Additional margin for dated contracts
    }
    
    @Override
    public BigDecimal calculatePnl(OpenPosition position, BigDecimal markPrice) {
        // Similar to perpetual but with expiry considerations
        BigDecimal priceDiff = position.getSide() == PositionSide.LONG 
            ? markPrice.subtract(position.getEntryPrice())
            : position.getEntryPrice().subtract(markPrice);
        return priceDiff.multiply(position.getQuantity()).multiply(getContractSize());
    }
}


///////////////////////////////////////////////////////////////////////////////////////////////////////////////////////



@SpringBootApplication
@EnableDiscoveryClient
public class ApiGatewayApplication {
    public static void main(String[] args) {
        SpringApplication.run(ApiGatewayApplication.class, args);
    }
    
    @Bean
    public RouteLocator customRouteLocator(RouteLocatorBuilder builder) {
        return builder.routes()
            .route("order-service", r -> r.path("/orders/**")
                .uri("lb://order-service"))
            .route("market-data", r -> r.path("/market/**")
                .uri("lb://market-data-service"))
            .build();
    }
}



///////////////////////////////////////////////////////////////////////////////////////////////////////////////

@Service
public class OrderServiceImpl implements OrderService {
    
    private final OrderRepository orderRepository;
    private final KafkaTemplate<String, OrderEvent> kafkaTemplate;
    
    @Autowired
    public OrderServiceImpl(OrderRepository orderRepository, 
                          KafkaTemplate<String, OrderEvent> kafkaTemplate) {
        this.orderRepository = orderRepository;
        this.kafkaTemplate = kafkaTemplate;
    }
    
    @Override
    @Transactional
    public Order createOrder(OrderRequest request) {
        // Validate order
        Order order = convertToOrder(request);
        orderRepository.save(order);
        
        // Publish order event
        OrderEvent event = new OrderEvent(order, OrderEventType.CREATED);
        kafkaTemplate.send("order-events", event);
        
        return order;
    }
    
    // Other order management methods...
}

@RestController
@RequestMapping("/api/orders")
public class OrderController {
    
    @Autowired
    private OrderService orderService;
    
    @PostMapping
    public ResponseEntity<Order> createOrder(@RequestBody OrderRequest request) {
        Order order = orderService.createOrder(request);
        return ResponseEntity.ok(order);
    }
    
    // Other endpoints...
}



//////////////////////////////////////////////////////////////////////////

@Service
public class MatchingEngineService {
    
    private final OrderBook orderBook;
    private final TradeRepository tradeRepository;
    private final KafkaTemplate<String, TradeEvent> tradeKafkaTemplate;
    
    public MatchingEngineService(OrderBook orderBook, 
                               TradeRepository tradeRepository,
                               KafkaTemplate<String, TradeEvent> tradeKafkaTemplate) {
        this.orderBook = orderBook;
        this.tradeRepository = tradeRepository;
        this.tradeKafkaTemplate = tradeKafkaTemplate;
    }
    
    @KafkaListener(topics = "order-events")
    public void processOrderEvent(OrderEvent event) {
        Order order = event.getOrder();
        orderBook.addOrder(order);
        
        // Match orders
        List<Trade> trades = orderBook.matchOrders();
        
        // Process trades
        trades.forEach(trade -> {
            tradeRepository.save(trade);
            tradeKafkaTemplate.send("trade-events", new TradeEvent(trade));
        });
    }
}






////////////////////////////////////////////////////////////////////////



@Service
public class MarketDataServiceImpl implements MarketDataService {
    
    private final Map<String, PriceFeed> priceFeeds = new ConcurrentHashMap<>();
    private final SimpMessagingTemplate messagingTemplate;
    
    @Autowired
    public MarketDataServiceImpl(SimpMessagingTemplate messagingTemplate) {
        this.messagingTemplate = messagingTemplate;
    }
    
    @Override
    public void subscribe(String symbol, PriceListener listener) {
        PriceFeed feed = priceFeeds.computeIfAbsent(symbol, 
            s -> new PriceFeed(s, this::onPriceUpdate));
        feed.addListener(listener);
    }
    
    private void onPriceUpdate(PriceUpdate update) {
        // Broadcast via WebSocket
        messagingTemplate.convertAndSend("/topic/market-data/" + update.getSymbol(), update);
        
        // Publish to Kafka for other services
        kafkaTemplate.send("price-updates", update);
    }
}





///////////////////////////////////////////////////////////////



@Service
public class RiskManagementService {
    
    private final PositionService positionService;
    private final AccountRepository accountRepository;
    
    @KafkaListener(topics = "order-events")
    public void validateOrder(OrderEvent event) {
        Order order = event.getOrder();
        Account account = accountRepository.findById(order.getAccountId())
            .orElseThrow(() -> new RuntimeException("Account not found"));
        
        // Check margin requirements
        if (!hasSufficientMargin(account, order)) {
            throw new RiskViolationException("Insufficient margin");
        }
        
        // Check position limits
        if (exceedsPositionLimits(account, order)) {
            throw new RiskViolationException("Position limit exceeded");
        }
    }
    
    // Risk calculation methods...
}





/////////////////////////////////////////////////////////////////////////



@SpringBootApplication
@EnableEurekaServer
public class ServiceRegistryApplication {
    public static void main(String[] args) {
        SpringApplication.run(ServiceRegistryApplication.class, args);
    }
}


///////////////////////////////


@SpringBootApplication
@EnableConfigServer
public class ConfigServerApplication {
    public static void main(String[] args) {
        SpringApplication.run(ConfigServerApplication.class, args);
    }
}



///////////////////////


public class OrderEvent implements Serializable {
    private String eventId;
    private Order order;
    private OrderEventType eventType;
    private Instant timestamp;
    // getters, setters, constructors
}

public class TradeEvent implements Serializable {
    private String eventId;
    private Trade trade;
    private Instant timestamp;
    // getters, setters, constructors
}

public class PriceUpdate implements Serializable {
    private String symbol;
    private BigDecimal bid;
    private BigDecimal ask;
    private BigDecimal last;
    private Instant timestamp;
    // getters, setters, constructors
}


//////////////////////////


@Entity
@Table(name = "orders")
public class Order {
    @Id
    private String id;
    private String accountId;
    private String symbol;
    private OrderType type;
    private OrderSide side;
    private BigDecimal price;
    private BigDecimal quantity;
    private OrderStatus status;
    private Instant createdAt;
    // getters, setters
}

@Entity
@Table(name = "trades")
public class Trade {
    @Id
    private String id;
    private String symbol;
    private BigDecimal price;
    private BigDecimal quantity;
    private String buyOrderId;
    private String sellOrderId;
    private Instant executedAt;
    // getters, setters
}


///////////////////
















