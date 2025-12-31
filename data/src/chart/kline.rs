use exchange::{
    Kline, Trade,
    util::{Price, PriceStep},
};
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

use crate::aggr::time::DataPoint;

// K线数据点结构体
// 存储K线数据及其对应的footprint（订单流）数据
#[derive(Clone)]
pub struct KlineDataPoint {
    pub kline: Kline,           // K线数据（开盘、收盘、最高、最低价等）
    pub footprint: KlineTrades, // footprint/订单流数据，记录每个价格水平的买卖量
}

// KlineDataPoint的实现块
// 为KlineDataPoint结构体实现方法
impl KlineDataPoint {
    // 计算在指定价格范围内最大的簇（cluster）数量
    // - cluster_kind: 簇的类型（买卖盘、成交量分布、Delta分布）
    // - highest/lowest: 价格范围
    // 使用泛型函数f处理不同的计算逻辑
    pub fn max_cluster_qty(&self, cluster_kind: ClusterKind, highest: Price, lowest: Price) -> f32 {
        match cluster_kind {
            ClusterKind::BidAsk => self.footprint.max_qty_by(highest, lowest, f32::max),
            ClusterKind::DeltaProfile => self
                .footprint
                .max_qty_by(highest, lowest, |buy, sell| (buy - sell).abs()),
            ClusterKind::VolumeProfile => {
                self.footprint
                    .max_qty_by(highest, lowest, |buy, sell| buy + sell)
            }
        }
    }

    // 将一笔交易添加到最近的bin（价格区间）
    // &Trade 表示借用Trade结构体的不可变引用
    // PriceStep 是价格步长，控制bin的精度
    pub fn add_trade(&mut self, trade: &Trade, step: PriceStep) {
        self.footprint.add_trade_to_nearest_bin(trade, step);
    }

    // 获取控制点（POC - Point of Control）的价格
    // POC是成交量最大的价格水平
    // Option<Price> 是Rust的可选类型，表示可能没有POC（当没有交易时）
    pub fn poc_price(&self) -> Option<Price> {
        self.footprint.poc_price()
    }

    // 设置POC的状态（是否被突破）
    pub fn set_poc_status(&mut self, status: NPoc) {
        self.footprint.set_poc_status(status);
    }

    // 清除所有交易数据
    pub fn clear_trades(&mut self) {
        self.footprint.clear();
    }

    // 计算POC（找到成交量最大的价格）
    pub fn calculate_poc(&mut self) {
        self.footprint.calculate_poc();
    }

    // 获取最后一笔交易的时间
    pub fn last_trade_time(&self) -> Option<u64> {
        self.footprint.last_trade_t()
    }

    // 获取第一笔交易的时间
    pub fn first_trade_time(&self) -> Option<u64> {
        self.footprint.first_trade_t()
    }
}

// 为KlineDataPoint实现DataPoint trait
// trait 是Rust的接口，定义了类型必须实现的行为
// 这行代码表示：为KlineDataPoint类型实现DataPoint trait的所有方法
impl DataPoint for KlineDataPoint {
    fn add_trade(&mut self, trade: &Trade, step: PriceStep) {
        self.add_trade(trade, step);
    }

    fn clear_trades(&mut self) {
        self.clear_trades();
    }

    fn last_trade_time(&self) -> Option<u64> {
        self.last_trade_time()
    }

    fn first_trade_time(&self) -> Option<u64> {
        self.first_trade_time()
    }

    fn last_price(&self) -> Price {
        self.kline.close
    }

    fn kline(&self) -> Option<&Kline> {
        Some(&self.kline)
    }

    fn value_high(&self) -> Price {
        self.kline.high
    }

    fn value_low(&self) -> Price {
        self.kline.low
    }
}

// 分组交易数据结构
// 存储在特定价格水平上聚合的交易信息
#[derive(Debug, Clone, Default)]
pub struct GroupedTrades {
    pub buy_qty: f32,       // 买入总量
    pub sell_qty: f32,      // 卖出总量
    pub first_time: u64,    // 第一笔交易的时间戳
    pub last_time: u64,     // 最后一笔交易的时间戳
    pub buy_count: usize,   // 买入交易笔数（usize是平台相关的无符号整数类型）
    pub sell_count: usize,  // 卖出交易笔数
}

// GroupedTrades的实现块
impl GroupedTrades {
    // 构造函数，从单笔交易创建GroupedTrades实例
    // - &Trade 表示借用Trade结构体的不可变引用
    // 根据交易的买卖方向初始化对应的字段
    fn new(trade: &Trade) -> Self {
        Self {
            buy_qty: if trade.is_sell { 0.0 } else { trade.qty },  // 如果是卖单，buy_qty为0
            sell_qty: if trade.is_sell { trade.qty } else { 0.0 }, // 如果是买单，sell_qty为0
            first_time: trade.time,
            last_time: trade.time,
            buy_count: if trade.is_sell { 0 } else { 1 },  // 买入笔数
            sell_count: if trade.is_sell { 1 } else { 0 }, // 卖出笔数
        }
    }

    // 向现有的GroupedTrades中添加一笔交易
    fn add_trade(&mut self, trade: &Trade) {
        if trade.is_sell {
            self.sell_qty += trade.qty;   // 累加卖出量
            self.sell_count += 1;         // 卖出笔数+1
        } else {
            self.buy_qty += trade.qty;    // 累加买入量
            self.buy_count += 1;          // 买入笔数+1
        }
        self.last_time = trade.time;      // 更新最后交易时间
    }

    // 计算总成交量（买入+卖出）
    pub fn total_qty(&self) -> f32 {
        self.buy_qty + self.sell_qty
    }

    // 计算净成交量（买入-卖出），即Delta
    pub fn delta_qty(&self) -> f32 {
        self.buy_qty - self.sell_qty
    }
}

// K线交易策略结构体
// 存储K线周期内的所有交易数据
#[derive(Debug, Clone, Default)]
pub struct KlineTrades {
    pub trades: FxHashMap<Price, GroupedTrades>,  // 映射：价格 -> 该价格的交易分组
    pub poc: Option<PointOfControl>,             // 控制点POC（可选，可能没有）
}

// KlineTrades的实现块
impl KlineTrades {
    // 构造函数，创建空的KlineTrades实例
    pub fn new() -> Self {
        Self {
            trades: FxHashMap::default(),  // 使用default()创建默认的空HashMap
            poc: None,
        }
    }

    // 获取第一笔交易的时间
    // Option<u64> 是Rust的可选类型，如果有交易返回Some(time)，否则返回None
    pub fn first_trade_t(&self) -> Option<u64> {
        self.trades.values().map(|group| group.first_time).min()
    }

    // 获取最后一笔交易的时间
    pub fn last_trade_t(&self) -> Option<u64> {
        self.trades.values().map(|group| group.last_time).max()
    }

    /// 基于买卖方向的装仓方式将交易添加到bin
    /// 专为订单簿/报价设计：卖单向下取整，买单向上取整
    /// 在bin边界处会引如方向偏差，不应用于OHLC/footprint聚合
    pub fn add_trade_to_side_bin(&mut self, trade: &Trade, step: PriceStep) {
        let price = trade.price.round_to_side_step(trade.is_sell, step);

        // entry() 是HashMap的方法，获取指定key的条目
        // and_modify() 如果key存在，则修改对应的值
        // or_insert_with() 如果key不存在，则插入新值
        self.trades
            .entry(price)
            .and_modify(|group| group.add_trade(trade))
            .or_insert_with(|| GroupedTrades::new(trade));
    }

    /// 使用最近步长倍数方式添加交易到bin（无视方向）
    ///平局中点向上取整到更高的倍数
    /// 专为footprint/OHLC交易聚合设计
    pub fn add_trade_to_nearest_bin(&mut self, trade: &Trade, step: PriceStep) {
        let price = trade.price.round_to_step(step);

        // 使用entry API优雅地处理"存在则修改，不存在则插入"逻辑
        // 比先contains_key()再insert()更高效，只需一次哈希查找
        self.trades
            .entry(price)
            .and_modify(|group| group.add_trade(trade))
            .or_insert_with(|| GroupedTrades::new(trade));
    }

    // 在指定价格范围内，使用自定义函数计算最大数量
    // - F: 泛型参数，表示一个函数类型（Rust的函数式编程特性）
    // - where 子句：对泛型参数的约束，F必须实现Fn(f32, f32) -> f32 trait
    // 这意味着F是一个接收两个f32参数并返回f32的函数
    pub fn max_qty_by<F>(&self, highest: Price, lowest: Price, f: F) -> f32
    where
        F: Fn(f32, f32) -> f32,
    {
        let mut max_qty: f32 = 0.0;
        // 迭代trades HashMap
        // (price, group) 是模式匹配，解构元组
        for (price, group) in &self.trades {
            // 只处理在价格范围内的交易
            if *price >= lowest && *price <= highest {
                max_qty = max_qty.max(f(group.buy_qty, group.sell_qty));
            }
        }
        max_qty
    }

    // 计算POC（控制点）- 成交量最大的价格
    pub fn calculate_poc(&mut self) {
        // 如果trades为空，直接返回（提前返回模式）
        if self.trades.is_empty() {
            return;
        }

        let mut max_volume = 0.0;
        let mut poc_price = Price::from_f32(0.0);

        // 遍历所有交易，找出最大成交量对应的价格
        // &self.trades 借用trades的不可变引用
        for (price, group) in &self.trades {
            let total_volume = group.total_qty();
            if total_volume > max_volume {
                max_volume = total_volume;
                poc_price = *price;
            }
        }

        // 创建并存储PointOfControl实例
        self.poc = Some(PointOfControl {
            price: poc_price,
            volume: max_volume,
            status: NPoc::default(),
        });
    }

    // 设置POC的状态
    pub fn set_poc_status(&mut self, status: NPoc) {
        // if let 语法：安全解包Option，只处理Some的情况
        if let Some(poc) = &mut self.poc {
            poc.status = status;
        }
    }

    // 获取POC的价格
    pub fn poc_price(&self) -> Option<Price> {
        // map() 是Option的方法，如果有Some值，应用函数并返回新的Option
        self.poc.map(|poc| poc.price)
    }

    // 清除所有数据
    pub fn clear(&mut self) {
        self.trades.clear();     // 清空HashMap
        self.poc = None;         // 重置POC为None
    }
}

// K线图表类型枚举
// 定义支持的图表类型：普通K线图或Footprint图
// #[derive(...)] 多个派生宏：
// - Debug: 用于打印调试信息
// - Clone: 可克隆
// - PartialEq/Eq: 可比较相等性
// - Default: 提供默认值
// - Deserialize/Serialize: 序列化反序列化支持（serde）
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize, Serialize)]
pub enum KlineChartKind {
    #[default]
    Candles,  // 普通K线图
    Footprint {  // Footprint图（订单流图）
        clusters: ClusterKind,  // 簇的类型
        #[serde(default)]  // 反序列化时使用默认值如果字段缺失
        scaling: ClusterScaling,  // 缩放模式
        studies: Vec<FootprintStudy>,  // 研究指标集合（Vec是Rust的动态数组）
    },
}

impl KlineChartKind {
    pub fn min_scaling(&self) -> f32 {
        match self {
            KlineChartKind::Footprint { .. } => 0.4,
            KlineChartKind::Candles => 0.6,
        }
    }

    pub fn max_scaling(&self) -> f32 {
        match self {
            KlineChartKind::Footprint { .. } => 2.0,
            KlineChartKind::Candles => 2.5,
        }
    }

    pub fn max_cell_width(&self) -> f32 {
        match self {
            KlineChartKind::Footprint { .. } => 360.0,
            KlineChartKind::Candles => 16.0,
        }
    }

    pub fn min_cell_width(&self) -> f32 {
        match self {
            KlineChartKind::Footprint { .. } => 80.0,
            KlineChartKind::Candles => 1.0,
        }
    }

    pub fn max_cell_height(&self) -> f32 {
        match self {
            KlineChartKind::Footprint { .. } => 90.0,
            KlineChartKind::Candles => 8.0,
        }
    }

    pub fn min_cell_height(&self) -> f32 {
        match self {
            KlineChartKind::Footprint { .. } => 1.0,
            KlineChartKind::Candles => 0.001,
        }
    }

    pub fn default_cell_width(&self) -> f32 {
        match self {
            KlineChartKind::Footprint { .. } => 80.0,
            KlineChartKind::Candles => 4.0,
        }
    }
}

// 簇（Cluster）类型枚举
// 定义Footprint图表中显示的簇的类型
// Copy trait 表示可以按位拷贝（类似C语言的memcpy），通常用于小的值类型
// Copy 和 Clone 的区别：Copy 是隐式的，Clone 需要显式调用 .clone()
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default, Deserialize, Serialize)]
pub enum ClusterKind {
    #[default]
    BidAsk,         // 买卖盘分开显示（左卖右买）
    VolumeProfile,  // 成交量分布（买卖合并显示）
    DeltaProfile,   // Delta分布（净成交量）
}

// ClusterKind的实现块
impl ClusterKind {
    // 常量数组，包含所有簇类型
    // 注意：Rust中数组的长度是编译时确定的（类型的一部分）
    // 这里 [ClusterKind; 3] 表示包含3个ClusterKind元素的数组
    pub const ALL: [ClusterKind; 3] = [
        ClusterKind::BidAsk,
        ClusterKind::VolumeProfile,
        ClusterKind::DeltaProfile,
    ];
}

// 为ClusterKind实现Display trait（格式化打印）
// 允许使用 {} 格式化该类型
// &mut std::fmt::Formatter<'_> 中的 '_ 是匿名生命周期
// 表示 Formatter 的生命周期由编译器自动推断
impl std::fmt::Display for ClusterKind {
    // fmt 方法必须返回 Result<&std::fmt::Result>，这是错误处理的一部分
    // std::fmt::Result 实际上是 type Result<(), std::fmt::Error> 的重命名
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // write! 宏与println!类似，但写入到Formatter
            ClusterKind::BidAsk => write!(f, "Bid/Ask"),
            ClusterKind::VolumeProfile => write!(f, "Volume Profile"),
            ClusterKind::DeltaProfile => write!(f, "Delta Profile"),
        }
    }
}

#[derive(Debug, Default, Copy, Clone, PartialEq, Deserialize, Serialize)]
pub struct Config {}

#[derive(Default, Clone, Copy, Debug, PartialEq, Deserialize, Serialize)]
pub enum ClusterScaling {
    #[default]
    /// Scale based on the maximum quantity in the visible range.
    VisibleRange,
    /// Blend global VisibleRange and per-cluster Individual using a weight in [0.0, 1.0].
    /// weight = fraction of global contribution (1.0 == all-global, 0.0 == all-individual).
    Hybrid { weight: f32 },
    /// Scale based only on the maximum quantity inside the datapoint (per-candle).
    Datapoint,
}

impl ClusterScaling {
    pub const ALL: [ClusterScaling; 3] = [
        ClusterScaling::VisibleRange,
        ClusterScaling::Hybrid { weight: 0.2 },
        ClusterScaling::Datapoint,
    ];
}

impl std::fmt::Display for ClusterScaling {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClusterScaling::VisibleRange => write!(f, "Visible Range"),
            ClusterScaling::Hybrid { weight } => write!(f, "Hybrid (weight: {:.2})", weight),
            ClusterScaling::Datapoint => write!(f, "Per-candle"),
        }
    }
}

impl std::cmp::Eq for ClusterScaling {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub enum FootprintStudy {
    NPoC {
        lookback: usize,
    },
    Imbalance {
        threshold: usize,
        color_scale: Option<usize>,
        ignore_zeros: bool,
    },
}

impl FootprintStudy {
    pub fn is_same_type(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (FootprintStudy::NPoC { .. }, FootprintStudy::NPoC { .. })
                | (
                    FootprintStudy::Imbalance { .. },
                    FootprintStudy::Imbalance { .. }
                )
        )
    }
}

impl FootprintStudy {
    pub const ALL: [FootprintStudy; 2] = [
        FootprintStudy::NPoC { lookback: 80 },
        FootprintStudy::Imbalance {
            threshold: 200,
            color_scale: Some(400),
            ignore_zeros: true,
        },
    ];
}

impl std::fmt::Display for FootprintStudy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FootprintStudy::NPoC { .. } => write!(f, "Naked Point of Control"),
            FootprintStudy::Imbalance { .. } => write!(f, "Imbalance"),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PointOfControl {
    pub price: Price,
    pub volume: f32,
    pub status: NPoc,
}

impl Default for PointOfControl {
    fn default() -> Self {
        Self {
            price: Price::from_f32(0.0),
            volume: 0.0,
            status: NPoc::default(),
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum NPoc {
    #[default]
    None,
    Naked,
    Filled {
        at: u64,
    },
}

impl NPoc {
    pub fn filled(&mut self, at: u64) {
        *self = NPoc::Filled { at };
    }

    pub fn unfilled(&mut self) {
        *self = NPoc::Naked;
    }
}
