use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Coupon {
    id: u64,
    name: String,
    category: CouponCategory,
    discount_amount: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum CouponCategory {
    Discount,
    FreeShipping,
    Cashback,
    Gift,
    BOGO,
}

#[derive(Debug, Clone, Deserialize)]
struct OptimizeRequest {
    coupon_ids: Vec<u64>,
}

#[derive(Debug, Clone, Serialize)]
struct OptimizeResponse {
    total_discount: u64,
    optimal_coupons: Vec<Coupon>,
    excluded_coupons: Vec<Coupon>,
}

#[derive(Debug, Clone, Deserialize)]
struct ValidateRequest {
    coupon_ids: Vec<u64>,
}

#[derive(Debug, Clone, Serialize)]
struct ValidateResponse {
    valid: bool,
    conflicts: Vec<ConflictDetail>,
}

#[derive(Debug, Clone, Serialize)]
struct ConflictDetail {
    coupon_a: u64,
    coupon_a_name: String,
    coupon_b: u64,
    coupon_b_name: String,
    reason: String,
}

#[derive(Debug, Clone, Serialize)]
struct ErrorResponse {
    error: String,
}

struct AppState {
    coupons: HashMap<u64, Coupon>,
    exclusion_rules: HashMap<CouponCategory, HashSet<CouponCategory>>,
}

impl AppState {
    fn new() -> Self {
        let mut coupons = HashMap::new();

        let seed = vec![
            Coupon { id: 1, name: "满100减20".into(), category: CouponCategory::Discount, discount_amount: 20 },
            Coupon { id: 2, name: "全场8折".into(), category: CouponCategory::Discount, discount_amount: 80 },
            Coupon { id: 3, name: "包邮券".into(), category: CouponCategory::FreeShipping, discount_amount: 15 },
            Coupon { id: 4, name: "返现5元".into(), category: CouponCategory::Cashback, discount_amount: 5 },
            Coupon { id: 5, name: "赠品券".into(), category: CouponCategory::Gift, discount_amount: 30 },
            Coupon { id: 6, name: "买一送一".into(), category: CouponCategory::BOGO, discount_amount: 100 },
            Coupon { id: 7, name: "满200减50".into(), category: CouponCategory::Discount, discount_amount: 50 },
            Coupon { id: 8, name: "返现10元".into(), category: CouponCategory::Cashback, discount_amount: 10 },
        ];
        for c in seed {
            coupons.insert(c.id, c);
        }

        let mut exclusion_rules: HashMap<CouponCategory, HashSet<CouponCategory>> = HashMap::new();

        exclusion_rules.insert(CouponCategory::Discount, {
            let mut s = HashSet::new();
            s.insert(CouponCategory::Discount);
            s.insert(CouponCategory::Cashback);
            s.insert(CouponCategory::BOGO);
            s
        });
        exclusion_rules.insert(CouponCategory::Cashback, {
            let mut s = HashSet::new();
            s.insert(CouponCategory::Discount);
            s.insert(CouponCategory::Cashback);
            s.insert(CouponCategory::BOGO);
            s.insert(CouponCategory::Gift);
            s
        });
        exclusion_rules.insert(CouponCategory::BOGO, {
            let mut s = HashSet::new();
            s.insert(CouponCategory::Discount);
            s.insert(CouponCategory::Cashback);
            s.insert(CouponCategory::BOGO);
            s.insert(CouponCategory::Gift);
            s
        });
        exclusion_rules.insert(CouponCategory::Gift, {
            let mut s = HashSet::new();
            s.insert(CouponCategory::Cashback);
            s.insert(CouponCategory::BOGO);
            s.insert(CouponCategory::Gift);
            s
        });
        exclusion_rules.insert(CouponCategory::FreeShipping, HashSet::new());

        Self { coupons, exclusion_rules }
    }

    fn check_conflicts(&self, coupon_ids: &[u64]) -> Result<Vec<ConflictDetail>, String> {
        let mut missing = Vec::new();
        let mut selected: Vec<&Coupon> = Vec::new();
        for &id in coupon_ids {
            match self.coupons.get(&id) {
                Some(c) => selected.push(c),
                None => missing.push(id),
            }
        }
        if !missing.is_empty() {
            return Err(format!("优惠券不存在: {:?}", missing));
        }

        let mut conflicts = Vec::new();
        let mut seen_pairs: HashSet<(u64, u64)> = HashSet::new();

        for i in 0..selected.len() {
            for j in (i + 1)..selected.len() {
                let a = &selected[i];
                let b = &selected[j];

                let pair = if a.id < b.id {
                    (a.id, b.id)
                } else {
                    (b.id, a.id)
                };
                if seen_pairs.contains(&pair) {
                    continue;
                }

                let a_excludes_b = self
                    .exclusion_rules
                    .get(&a.category)
                    .map_or(false, |s| s.contains(&b.category));

                let b_excludes_a = self
                    .exclusion_rules
                    .get(&b.category)
                    .map_or(false, |s| s.contains(&a.category));

                if a_excludes_b || b_excludes_a {
                    seen_pairs.insert(pair);
                    conflicts.push(ConflictDetail {
                        coupon_a: a.id,
                        coupon_a_name: a.name.clone(),
                        coupon_b: b.id,
                        coupon_b_name: b.name.clone(),
                        reason: format!(
                            "\"{}\"({:?}) 与 \"{}\"({:?}) 互斥",
                            a.name, a.category, b.name, b.category
                        ),
                    });
                }
            }
        }

        Ok(conflicts)
    }

    fn has_conflicts(&self, coupons: &[&Coupon]) -> bool {
        for i in 0..coupons.len() {
            for j in (i + 1)..coupons.len() {
                let a = &coupons[i];
                let b = &coupons[j];
                let a_excludes_b = self
                    .exclusion_rules
                    .get(&a.category)
                    .map_or(false, |s| s.contains(&b.category));
                let b_excludes_a = self
                    .exclusion_rules
                    .get(&b.category)
                    .map_or(false, |s| s.contains(&a.category));
                if a_excludes_b || b_excludes_a {
                    return true;
                }
            }
        }
        false
    }

    fn optimize_coupons(&self, coupon_ids: &[u64]) -> Result<OptimizeResponse, String> {
        let mut missing = Vec::new();
        let mut selected: Vec<&Coupon> = Vec::new();
        for &id in coupon_ids {
            match self.coupons.get(&id) {
                Some(c) => selected.push(c),
                None => missing.push(id),
            }
        }
        if !missing.is_empty() {
            return Err(format!("优惠券不存在: {:?}", missing));
        }

        selected.sort_by_key(|c| c.id);
        selected.dedup_by_key(|c| c.id);

        let n = selected.len();
        if n == 0 {
            return Ok(OptimizeResponse {
                total_discount: 0,
                optimal_coupons: Vec::new(),
                excluded_coupons: Vec::new(),
            });
        }
        if n > 31 {
            return Err("优惠券数量过多（最多31张）".into());
        }

        let mut best_mask: u32 = 0;
        let mut best_total: u64 = 0;

        for mask in 0..(1u32 << n) {
            let mut subset = Vec::new();
            for i in 0..n {
                if mask & (1u32 << i) != 0 {
                    subset.push(selected[i]);
                }
            }
            if !self.has_conflicts(&subset) {
                let total: u64 = subset.iter().map(|c| c.discount_amount).sum();
                if total > best_total {
                    best_total = total;
                    best_mask = mask;
                }
            }
        }

        let mut optimal = Vec::new();
        let mut excluded = Vec::new();
        for i in 0..n {
            if best_mask & (1u32 << i) != 0 {
                optimal.push(selected[i].clone());
            } else {
                excluded.push(selected[i].clone());
            }
        }

        Ok(OptimizeResponse {
            total_discount: best_total,
            optimal_coupons: optimal,
            excluded_coupons: excluded,
        })
    }
}

async fn list_coupons(State(state): State<Arc<AppState>>) -> Json<Vec<Coupon>> {
    let mut list: Vec<Coupon> = state.coupons.values().cloned().collect();
    list.sort_by_key(|c| c.id);
    Json(list)
}

async fn validate_coupons(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ValidateRequest>,
) -> Result<(StatusCode, Json<ValidateResponse>), (StatusCode, Json<ErrorResponse>)> {
    if req.coupon_ids.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "coupon_ids 不能为空".into(),
            }),
        ));
    }

    let mut deduped = req.coupon_ids.clone();
    deduped.sort();
    deduped.dedup();

    match state.check_conflicts(&deduped) {
        Ok(conflicts) => {
            let valid = conflicts.is_empty();
            Ok((StatusCode::OK, Json(ValidateResponse { valid, conflicts })))
        }
        Err(e) => Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ErrorResponse { error: e }),
        )),
    }
}

async fn optimize_coupons(
    State(state): State<Arc<AppState>>,
    Json(req): Json<OptimizeRequest>,
) -> Result<(StatusCode, Json<OptimizeResponse>), (StatusCode, Json<ErrorResponse>)> {
    if req.coupon_ids.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "coupon_ids 不能为空".into(),
            }),
        ));
    }

    match state.optimize_coupons(&req.coupon_ids) {
        Ok(resp) => Ok((StatusCode::OK, Json(resp))),
        Err(e) => Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ErrorResponse { error: e }),
        )),
    }
}

#[tokio::main]
async fn main() {
    let state = Arc::new(AppState::new());

    let app = Router::new()
        .route("/coupons", get(list_coupons))
        .route("/coupons/validate", post(validate_coupons))
        .route("/coupons/optimize", post(optimize_coupons))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("服务器启动: http://0.0.0.0:3000");
    println!("  GET  /coupons          - 查看所有优惠券");
    println!("  POST /coupons/validate - 校验优惠券互斥");
    println!("  POST /coupons/optimize - 计算最优组合");
    axum::serve(listener, app).await.unwrap();
}
