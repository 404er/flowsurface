// ============================================================================
// 时间序列数据聚合模块
// 
// 这个模块实现了基于时间的数据聚合功能，是图表系统的核心
// 负责将实时交易数据按时间间隔（如 1分钟、5分钟）聚合成 K线或热力图数据点
// ============================================================================

use std::collections::BTreeMap;  // BTreeMap 是有序映射，按键排序

use crate::chart::Basis;
use crate::chart::heatmap::HeatmapDataPoint;
use crate::chart::kline::{ClusterKind, KlineDataPoint, KlineTrades, NPoc};

use exchange::util::{Price, PriceStep};
use exchange::{Kline, Timeframe, Trade};

/// ============================================================================
/// DataPoint trait - 数据点抽象接口
/// 
/// 定义所有图表数据点必须实现的通用操作
/// 这是多态的基础，允许不同类型的图表使用相同的聚合逻辑
/// 
/// Rust 特性说明：
/// - trait 类似其他语言的 interface，定义行为契约
/// - 实现 trait 的类型必须提供所有方法的具体实现
/// - &mut self 表示可变借用（可以修改 self）
/// - &Trade 表示不可变借用（只读访问，不转移所有权）
/// - Option<T> 表示可能不存在的值（类型安全的空值）
/// ============================================================================
pub trait DataPoint {
    /// 添加交易数据到当前数据点
    /// 
    /// # 参数
    /// - trade: 交易数据的不可变引用
    /// - step: 价格步长，用于价格分组
    fn add_trade(&mut self, trade: &Trade, step: PriceStep);

    /// 清空交易数据（保留 K线结构）
    fn clear_trades(&mut self);

    /// 获取最后一笔交易的时间戳（毫秒）
    /// 返回 None 表示没有交易数据
    fn last_trade_time(&self) -> Option<u64>;

    /// 获取第一笔交易的时间戳（毫秒）
    fn first_trade_time(&self) -> Option<u64>;

    /// 获取最新价格
    fn last_price(&self) -> Price;

    /// 获取 K线数据（如果存在）
    /// 
    /// Option<&Kline> 表示返回 Kline 的引用或 None
    /// & 生命周期与 self 绑定，防止悬垂引用
    fn kline(&self) -> Option<&Kline>;

    /// 获取周期内最高价
    fn value_high(&self) -> Price;

    /// 获取周期内最低价
    fn value_low(&self) -> Price;
}

/// ============================================================================
/// TimeSeries - 时间序列数据结构
/// 
/// 存储按时间排序的数据点集合，支持高效的范围查询
/// 
/// Rust 特性说明：
/// - 泛型参数 <D: DataPoint> 表示 D 必须实现 DataPoint trait
/// - 这是 trait bound（特征约束），在编译时保证类型安全
/// - pub 使字段公开可访问
/// - BTreeMap<K, V> 是 B树实现的有序映射，键按顺序排列
///   - 查询复杂度: O(log n)
///   - 范围查询高效（连续内存访问）
///   - 适合时间序列数据
/// ============================================================================
pub struct TimeSeries<D: DataPoint> {
    /// 数据点映射：时间戳 -> 数据点
    /// 键是 Unix 时间戳（毫秒），值是泛型数据点
    pub datapoints: BTreeMap<u64, D>,
    
    /// 时间间隔（如 1分钟、5分钟、1小时等）
    pub interval: Timeframe,
    
    /// 价格步长，用于价格分组和显示
    pub tick_size: PriceStep,
}

/// ============================================================================
/// TimeSeries 的通用方法实现
/// 
/// impl<D: DataPoint> 为所有实现 DataPoint 的类型提供统一方法
/// 这是 Rust 的泛型编程，在编译时单态化（生成特化版本）
/// ============================================================================
impl<D: DataPoint> TimeSeries<D> {
    /// 获取基准价格（最新数据点的收盘价）
    /// 
    /// 用于图表坐标系的价格基准点
    /// 
    /// # Rust 特性
    /// - self 是不可变借用
    /// - values() 返回迭代器（零成本抽象）
    /// - map_or() 处理 Option，提供默认值
    pub fn base_price(&self) -> Price {
        self.datapoints
            .values()                                    // 获取所有值的迭代器
            .last()                                      // 获取最后一个（Option<&D>）
            .map_or(Price::from_f32(0.0), DataPoint::last_price)  // 转换为 Price
    }

    /// 获取最新的时间戳
    /// 
    /// # 返回值
    /// - Some(timestamp): 最新时间戳（毫秒）
    /// - None: 数据集为空
    /// 
    /// # Rust 特性
    /// - copied() 将 &u64 复制为 u64（u64 实现了 Copy trait）
    /// - Copy trait 表示可以按位复制，不需要 clone()
    pub fn latest_timestamp(&self) -> Option<u64> {
        self.datapoints.keys().last().copied()
    }

    /// 获取最新的 K线数据
    /// 
    /// # Rust 特性
    /// - and_then() 是 Option 的 flatMap 操作
    /// - 返回的引用生命周期与 self 绑定
    pub fn latest_kline(&self) -> Option<&Kline> {
        self.datapoints.values().last().and_then(|dp| dp.kline())
    }

    /// 计算指定回看期的价格范围
    /// 
    /// 用于自动缩放图表 Y 轴
    /// 
    /// # 参数
    /// - lookback: 回看的数据点数量
    /// 
    /// # 返回值
    /// - (highest, lowest): 价格范围
    /// 
    /// # 算法复杂度
    /// - 时间: O(lookback)
    /// - 空间: O(1)
    /// 
    /// # Rust 特性
    /// - iter().rev() 创建反向迭代器
    /// - take(n) 限制迭代器元素数量
    /// - 模式匹配 if let 处理 Option
    pub fn price_scale(&self, lookback: usize) -> (Price, Price) {
        // rev() 反向遍历（从最新到最旧）
        let mut iter = self.datapoints.iter().rev().take(lookback);

        // 模式匹配：解构元组 (key, value)
        if let Some((_, first)) = iter.next() {
            let mut high = first.value_high();
            let mut low = first.value_low();

            // 遍历剩余数据点，更新最高/最低价
            for (_, dp) in iter {
                let value_high = dp.value_high();
                let value_low = dp.value_low();
                if value_high > high {
                    high = value_high;
                }
                if value_low < low {
                    low = value_low;
                }
            }

            (high, low)
        } else {
            // 空数据集返回零值
            (Price::from_f32(0.0), Price::from_f32(0.0))
        }
    }

    pub fn volume_data<'a>(&'a self) -> BTreeMap<u64, (f32, f32)>
    where
        BTreeMap<u64, (f32, f32)>: From<&'a TimeSeries<D>>,
    {
        self.into()
    }

    pub fn timerange(&self) -> (u64, u64) {
        let earliest = self.datapoints.keys().next().copied().unwrap_or(0);
        let latest = self.datapoints.keys().last().copied().unwrap_or(0);

        (earliest, latest)
    }

    pub fn min_max_price_in_range_prices(
        &self,
        earliest: u64,
        latest: u64,
    ) -> Option<(Price, Price)> {
        let mut it = self.datapoints.range(earliest..=latest);

        let (_, first) = it.next()?;
        let mut min_price = first.value_low();
        let mut max_price = first.value_high();

        for (_, dp) in it {
            let low = dp.value_low();
            let high = dp.value_high();
            if low < min_price {
                min_price = low;
            }
            if high > max_price {
                max_price = high;
            }
        }

        Some((min_price, max_price))
    }

    pub fn min_max_price_in_range(&self, earliest: u64, latest: u64) -> Option<(f32, f32)> {
        self.min_max_price_in_range_prices(earliest, latest)
            .map(|(min_p, max_p)| (min_p.to_f32(), max_p.to_f32()))
    }

    pub fn clear_trades(&mut self) {
        for data_point in self.datapoints.values_mut() {
            data_point.clear_trades();
        }
    }

    pub fn check_kline_integrity(
        &self,
        earliest: u64,
        latest: u64,
        interval: u64,
    ) -> Option<Vec<u64>> {
        let mut time = earliest;
        let mut missing_count = 0;

        while time < latest {
            if !self.datapoints.contains_key(&time) {
                missing_count += 1;
                break;
            }
            time += interval;
        }

        if missing_count > 0 {
            let mut missing_keys = Vec::with_capacity(((latest - earliest) / interval) as usize);
            let mut time = earliest;

            while time < latest {
                if !self.datapoints.contains_key(&time) {
                    missing_keys.push(time);
                }
                time += interval;
            }

            log::warn!(
                "Integrity check failed: missing {} klines",
                missing_keys.len()
            );
            return Some(missing_keys);
        }

        None
    }
}

/// ============================================================================
/// TimeSeries<KlineDataPoint> 的特化实现
/// 
/// 为 K线数据点提供专门的方法
/// 这些方法只对 KlineDataPoint 类型可用
/// 
/// Rust 特性：类型特化（Type Specialization）
/// ============================================================================
impl TimeSeries<KlineDataPoint> {
    /// 创建新的 K线时间序列
    /// 
    /// # 参数
    /// - interval: 时间间隔（如 1分钟、5分钟）
    /// - tick_size: 价格步长
    /// - klines: 初始 K线数据切片
    /// 
    /// # Rust 特性
    /// - &[Kline] 是切片引用，可以传递数组或 Vec
    /// - Self 是当前类型的别名
    pub fn new(interval: Timeframe, tick_size: PriceStep, klines: &[Kline]) -> Self {
        let mut timeseries = Self {
            datapoints: BTreeMap::new(),
            interval,
            tick_size,
        };

        timeseries.insert_klines(klines);
        timeseries
    }

    /// 克隆当前时间序列并添加交易数据
    /// 
    /// 用于不可变操作，返回新的时间序列
    /// 
    /// # Rust 特性
    /// - clone() 深度复制整个 BTreeMap
    /// - Rust 默认是移动语义，clone 是显式复制
    pub fn with_trades(&self, trades: &[Trade]) -> TimeSeries<KlineDataPoint> {
        let mut new_series = Self {
            datapoints: self.datapoints.clone(),  // 深度复制
            interval: self.interval,
            tick_size: self.tick_size,
        };

        new_series.insert_trades_or_create_bucket(trades);
        new_series
    }

    /// 插入 K线数据到时间序列
    /// 
    /// 更新现有 K线或创建新的数据点
    /// 
    /// # Rust 特性
    /// - entry() API 提供高效的插入/更新操作
    /// - or_insert_with() 使用闭包延迟初始化（只在需要时执行）
    /// - *kline 是复制操作（Kline 实现了 Copy trait）
    pub fn insert_klines(&mut self, klines: &[Kline]) {
        for kline in klines {
            // entry() 获取条目的可变引用或插入默认值
            let entry = self
                .datapoints
                .entry(kline.time)
                .or_insert_with(|| KlineDataPoint {
                    kline: *kline,  // 解引用并复制
                    footprint: KlineTrades::new(),
                });

            // 更新 K线数据（如果已存在）
            entry.kline = *kline;
        }

        // 更新 POC (Point of Control) 状态
        self.update_poc_status();
    }

    /// 插入交易数据，自动创建或更新 K线桶
    /// 
    /// 这是实时数据聚合的核心方法
    /// 
    /// # 算法流程
    /// 1. 将交易时间戳向下取整到间隔边界
    /// 2. 查找或创建对应时间的 K线
    /// 3. 将交易数据添加到 Footprint
    /// 4. 重新计算 POC (成交量最大价格)
    /// 
    /// # 性能优化
    /// - 使用 Vec 跟踪更新的时间戳，避免重复计算 POC
    /// - 批量处理，只在最后统一更新 POC
    /// 
    /// # Rust 特性
    /// - buffer.iter().for_each() 是函数式编程风格
    /// - 闭包捕获外部变量（aggr_time, updated_times）
    pub fn insert_trades_or_create_bucket(&mut self, buffer: &[Trade]) {
        if buffer.is_empty() {
            return;  // 提前返回，避免不必要的计算
        }
        
        let aggr_time = self.interval.to_milliseconds();
        let mut updated_times = Vec::new();  // 跟踪哪些时间桶被更新

        // 遍历所有交易
        buffer.iter().for_each(|trade| {
            // 时间戳向下取整到间隔边界
            // 例如：14:32:45 with 5分钟间隔 -> 14:30:00
            let rounded_time = (trade.time / aggr_time) * aggr_time;

            // 记录更新的时间戳（用于后续 POC 计算）
            if !updated_times.contains(&rounded_time) {
                updated_times.push(rounded_time);
            }

            // 获取或创建数据点
            let entry = self
                .datapoints
                .entry(rounded_time)
                .or_insert_with(|| KlineDataPoint {
                    kline: Kline {
                        time: rounded_time,
                        open: trade.price,    // 首笔交易价格作为开盘价
                        high: trade.price,
                        low: trade.price,
                        close: trade.price,
                        volume: (0.0, 0.0),
                    },
                    footprint: KlineTrades::new(),
                });

            // 添加交易数据到 Footprint
            entry.add_trade(trade, self.tick_size);
        });

        // 批量更新所有受影响的数据点的 POC
        for time in updated_times {
            if let Some(data_point) = self.datapoints.get_mut(&time) {
                data_point.calculate_poc();
            }
        }
    }

    pub fn insert_trades_existing_buckets(&mut self, buffer: &[Trade]) {
        if buffer.is_empty() {
            return;
        }
        let aggr_time = self.interval.to_milliseconds();
        let mut updated_times: Vec<u64> = Vec::new();

        for trade in buffer {
            let rounded_time = (trade.time / aggr_time) * aggr_time;

            if let Some(entry) = self.datapoints.get_mut(&rounded_time) {
                if !updated_times.contains(&rounded_time) {
                    updated_times.push(rounded_time);
                }
                entry.add_trade(trade, self.tick_size);
            }
        }

        for time in updated_times {
            if let Some(data_point) = self.datapoints.get_mut(&time) {
                data_point.calculate_poc();
            }
        }
    }

    pub fn change_tick_size(&mut self, tick_size: f32, raw_trades: &[Trade]) {
        self.tick_size = PriceStep::from_f32(tick_size);
        self.clear_trades();

        if !raw_trades.is_empty() {
            self.insert_trades_existing_buckets(raw_trades);
        }
    }

    pub fn update_poc_status(&mut self) {
        let updates = self
            .datapoints
            .iter()
            .filter_map(|(&time, dp)| dp.poc_price().map(|price| (time, price)))
            .collect::<Vec<_>>();

        for (current_time, poc_price) in updates {
            let mut npoc = NPoc::default();

            for (&next_time, next_dp) in self.datapoints.range((current_time + 1)..) {
                let next_dp_low = next_dp.kline.low.round_to_side_step(true, self.tick_size);
                let next_dp_high = next_dp.kline.high.round_to_side_step(false, self.tick_size);

                if next_dp_low <= poc_price && next_dp_high >= poc_price {
                    npoc.filled(next_time);
                    break;
                } else {
                    npoc.unfilled();
                }
            }

            if let Some(data_point) = self.datapoints.get_mut(&current_time) {
                data_point.set_poc_status(npoc);
            }
        }
    }

    pub fn suggest_trade_fetch_range(
        &self,
        visible_earliest: u64,
        visible_latest: u64,
    ) -> Option<(u64, u64)> {
        if self.datapoints.is_empty() {
            return None;
        }

        self.find_trade_gap()
            .and_then(|(last_t_before_gap, first_t_after_gap)| {
                if last_t_before_gap.is_none() && first_t_after_gap.is_none() {
                    return None;
                }
                let (data_earliest, data_latest) = self.timerange();

                let fetch_from = last_t_before_gap
                    .map_or(data_earliest, |t| t.saturating_add(1))
                    .max(visible_earliest);
                let fetch_to = first_t_after_gap
                    .map_or(data_latest, |t| t.saturating_sub(1))
                    .min(visible_latest);

                if fetch_from < fetch_to {
                    Some((fetch_from, fetch_to))
                } else {
                    None
                }
            })
    }

    fn find_trade_gap(&self) -> Option<(Option<u64>, Option<u64>)> {
        let empty_kline_time = self
            .datapoints
            .iter()
            .rev()
            .find(|(_, dp)| dp.footprint.trades.is_empty())
            .map(|(&time, _)| time);

        if let Some(target_time) = empty_kline_time {
            let last_t_before_gap = self
                .datapoints
                .range(..target_time)
                .rev()
                .find_map(|(_, dp)| dp.last_trade_time());

            let first_t_after_gap = self
                .datapoints
                .range(target_time + 1..)
                .find_map(|(_, dp)| dp.first_trade_time());

            Some((last_t_before_gap, first_t_after_gap))
        } else {
            None
        }
    }

    pub fn max_qty_ts_range(
        &self,
        cluster_kind: ClusterKind,
        earliest: u64,
        latest: u64,
        highest: Price,
        lowest: Price,
    ) -> f32 {
        let mut max_cluster_qty: f32 = 0.0;

        self.datapoints
            .range(earliest..=latest)
            .for_each(|(_, dp)| {
                max_cluster_qty =
                    max_cluster_qty.max(dp.max_cluster_qty(cluster_kind, highest, lowest));
            });

        max_cluster_qty
    }
}

impl TimeSeries<HeatmapDataPoint> {
    pub fn new(basis: Basis, tick_size: PriceStep) -> Self {
        let timeframe = match basis {
            Basis::Time(interval) => interval,
            Basis::Tick(_) => unimplemented!(),
        };

        Self {
            datapoints: BTreeMap::new(),
            interval: timeframe,
            tick_size,
        }
    }

    pub fn max_trade_qty_and_aggr_volume(&self, earliest: u64, latest: u64) -> (f32, f32) {
        let mut max_trade_qty = 0.0f32;
        let mut max_aggr_volume = 0.0f32;

        self.datapoints
            .range(earliest..=latest)
            .for_each(|(_, dp)| {
                let (mut buy_volume, mut sell_volume) = (0.0, 0.0);

                dp.grouped_trades.iter().for_each(|trade| {
                    max_trade_qty = max_trade_qty.max(trade.qty);

                    if trade.is_sell {
                        sell_volume += trade.qty;
                    } else {
                        buy_volume += trade.qty;
                    }
                });

                max_aggr_volume = max_aggr_volume.max(buy_volume + sell_volume);
            });

        (max_trade_qty, max_aggr_volume)
    }
}

impl From<&TimeSeries<KlineDataPoint>> for BTreeMap<u64, (f32, f32)> {
    /// Converts datapoints into a map of timestamps and volume data
    fn from(timeseries: &TimeSeries<KlineDataPoint>) -> Self {
        timeseries
            .datapoints
            .iter()
            .map(|(time, dp)| (*time, (dp.kline.volume.0, dp.kline.volume.1)))
            .collect()
    }
}
