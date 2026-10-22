use std::collections::BTreeMap;

use chrono::{DateTime, Datelike, Days, FixedOffset, Months, NaiveDate, Utc};

use crate::models::Transaction;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DateRange {
    start: NaiveDate,
    end: NaiveDate,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DateError {
    #[error("Data inválida. Use DD/MM/YYYY ou YYYY-MM-DD.")]
    InvalidDate,
    #[error("A data inicial deve ser anterior ou igual à final.")]
    ReversedRange,
}

impl DateRange {
    pub fn new(start: NaiveDate, end: NaiveDate) -> Result<Self, DateError> {
        if start > end {
            return Err(DateError::ReversedRange);
        }
        Ok(Self { start, end })
    }

    pub fn parse(start: &str, end: &str) -> Result<Self, DateError> {
        Self::new(parse_date(start)?, parse_date(end)?)
    }

    #[must_use]
    pub fn start(self) -> NaiveDate {
        self.start
    }

    #[must_use]
    pub fn end(self) -> NaiveDate {
        self.end
    }

    #[must_use]
    pub fn contains(self, date: NaiveDate) -> bool {
        self.start <= date && date <= self.end
    }

    #[must_use]
    pub fn query_start(self) -> String {
        self.start.format("%Y-%m-%d").to_string()
    }

    #[must_use]
    pub fn query_end(self) -> String {
        self.end.format("%Y-%m-%d").to_string()
    }
}

pub fn parse_date(value: &str) -> Result<NaiveDate, DateError> {
    let value = value.trim();
    let format = if value.len() == 10 && value.as_bytes()[2] == b'/' && value.as_bytes()[5] == b'/'
    {
        "%d/%m/%Y"
    } else if value.len() == 10 && value.as_bytes()[4] == b'-' && value.as_bytes()[7] == b'-' {
        "%Y-%m-%d"
    } else {
        return Err(DateError::InvalidDate);
    };
    NaiveDate::parse_from_str(value, format).map_err(|_| DateError::InvalidDate)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum PeriodKind {
    Month,
    Week,
    Quarter,
    Year,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Period {
    kind: PeriodKind,
    range: DateRange,
}

impl Default for Period {
    fn default() -> Self {
        Self::current_month(brasilia_today())
    }
}

impl Period {
    #[must_use]
    pub fn current_month(today: NaiveDate) -> Self {
        Self::month(today)
    }

    #[must_use]
    pub fn month(anchor: NaiveDate) -> Self {
        Self {
            kind: PeriodKind::Month,
            range: DateRange {
                start: anchor.with_day(1).expect("first day exists"),
                end: month_end(anchor),
            },
        }
    }

    #[must_use]
    pub fn week(anchor: NaiveDate) -> Self {
        let start = anchor
            .checked_sub_days(Days::new(u64::from(
                anchor.weekday().num_days_from_monday(),
            )))
            .unwrap_or(NaiveDate::MIN);
        Self {
            kind: PeriodKind::Week,
            range: DateRange {
                start,
                end: start
                    .checked_add_days(Days::new(6))
                    .unwrap_or(NaiveDate::MAX),
            },
        }
    }

    #[must_use]
    pub fn quarter(anchor: NaiveDate) -> Self {
        let month = ((anchor.month() - 1) / 3) * 3 + 1;
        let start = NaiveDate::from_ymd_opt(anchor.year(), month, 1).expect("quarter start exists");
        let last =
            NaiveDate::from_ymd_opt(anchor.year(), month + 2, 1).expect("quarter end exists");
        Self {
            kind: PeriodKind::Quarter,
            range: DateRange {
                start,
                end: month_end(last),
            },
        }
    }

    #[must_use]
    pub fn year(anchor: NaiveDate) -> Self {
        Self {
            kind: PeriodKind::Year,
            range: DateRange {
                start: NaiveDate::from_ymd_opt(anchor.year(), 1, 1).expect("year start exists"),
                end: NaiveDate::from_ymd_opt(anchor.year(), 12, 31).expect("year end exists"),
            },
        }
    }

    #[must_use]
    pub fn custom(range: DateRange) -> Self {
        Self {
            kind: PeriodKind::Custom,
            range,
        }
    }

    #[must_use]
    pub fn range(self) -> DateRange {
        self.range
    }

    #[must_use]
    pub fn label(self) -> String {
        const MONTHS: [&str; 12] = [
            "Janeiro",
            "Fevereiro",
            "Março",
            "Abril",
            "Maio",
            "Junho",
            "Julho",
            "Agosto",
            "Setembro",
            "Outubro",
            "Novembro",
            "Dezembro",
        ];
        let start = self.range.start;
        match self.kind {
            PeriodKind::Month => format!("{} {}", MONTHS[start.month0() as usize], start.year()),
            PeriodKind::Quarter => format!(
                "{}º trimestre {}",
                (start.month() - 1) / 3 + 1,
                start.year()
            ),
            PeriodKind::Year => start.year().to_string(),
            PeriodKind::Week | PeriodKind::Custom => format!(
                "{} a {}",
                start.format("%d/%m/%Y"),
                self.range.end.format("%d/%m/%Y")
            ),
        }
    }

    #[must_use]
    pub fn previous_month(self) -> Self {
        Self::month(
            self.range
                .start
                .checked_sub_months(Months::new(1))
                .unwrap_or(self.range.start),
        )
    }

    #[must_use]
    pub fn next_month(self) -> Self {
        Self::month(
            self.range
                .start
                .checked_add_months(Months::new(1))
                .unwrap_or(self.range.start),
        )
    }
}

fn month_end(date: NaiveDate) -> NaiveDate {
    (28..=31)
        .rev()
        .find_map(|day| date.with_day(day))
        .expect("month has at least 28 days")
}

fn brasilia_offset() -> FixedOffset {
    FixedOffset::west_opt(3 * 60 * 60).expect("Brasília UTC-3 offset exists")
}

#[must_use]
pub fn brasilia_today() -> NaiveDate {
    Utc::now().with_timezone(&brasilia_offset()).date_naive()
}

#[must_use]
pub fn transaction_date(transaction: &Transaction) -> Option<NaiveDate> {
    let value = transaction.transacted_at.as_deref()?.trim();
    parse_date(value).ok().or_else(|| {
        DateTime::parse_from_rfc3339(value)
            .ok()
            .map(|date| date.with_timezone(&brasilia_offset()).date_naive())
    })
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FlowTotals {
    pub income: i128,
    pub expenses: i128,
    pub count: usize,
}

impl FlowTotals {
    #[must_use]
    pub fn result(self) -> i128 {
        self.income - self.expenses
    }

    fn with_amount(self, amount: i128, income: bool) -> Option<Self> {
        Some(Self {
            income: if income {
                self.income.checked_add(amount)?
            } else {
                self.income
            },
            expenses: if income {
                self.expenses
            } else {
                self.expenses.checked_add(amount)?
            },
            count: self.count.checked_add(1)?,
        })
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PeriodSummary {
    pub currencies: BTreeMap<String, FlowTotals>,
    pub categories: BTreeMap<String, BTreeMap<String, FlowTotals>>,
    pub invalid_amounts: usize,
}

#[must_use]
pub fn summarize(transactions: &[Transaction]) -> PeriodSummary {
    summarize_iter(transactions.iter())
}

fn summarize_iter<'a>(transactions: impl Iterator<Item = &'a Transaction>) -> PeriodSummary {
    let mut summary = PeriodSummary::default();
    for transaction in transactions {
        if transaction.ignored {
            continue;
        }
        let income = match transaction.tx_type.to_ascii_lowercase().as_str() {
            "credit" => true,
            "debit" => false,
            _ => continue,
        };
        let Some(amount) = decimal_cents(&transaction.amount).and_then(i128::checked_abs) else {
            summary.invalid_amounts += 1;
            continue;
        };
        let currency = transaction
            .currency_code
            .as_deref()
            .map(str::trim)
            .filter(|currency| !currency.is_empty())
            .unwrap_or("?")
            .to_uppercase();
        let category = transaction.category_id.as_deref().unwrap_or("");
        let totals = summary
            .currencies
            .get(&currency)
            .copied()
            .unwrap_or_default();
        let category_totals = summary
            .categories
            .get(category)
            .and_then(|currencies| currencies.get(&currency))
            .copied()
            .unwrap_or_default();
        let (Some(totals), Some(category_totals)) = (
            totals.with_amount(amount, income),
            category_totals.with_amount(amount, income),
        ) else {
            summary.invalid_amounts += 1;
            continue;
        };
        summary.currencies.insert(currency.clone(), totals);
        summary
            .categories
            .entry(category.into())
            .or_default()
            .insert(currency, category_totals);
    }
    summary
}

fn decimal_cents(value: &str) -> Option<i128> {
    let value = value.trim();
    let negative = value.starts_with('-');
    let unsigned = value.strip_prefix(['-', '+']).unwrap_or(value);
    let (whole, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
    if whole.is_empty()
        || !whole.bytes().all(|digit| digit.is_ascii_digit())
        || !fraction.bytes().all(|digit| digit.is_ascii_digit())
        || fraction.bytes().skip(2).any(|digit| digit != b'0')
    {
        return None;
    }
    let mut cents = whole.parse::<i128>().ok()?.checked_mul(100)?;
    let mut digits = fraction.bytes();
    cents = cents.checked_add(i128::from(digits.next().unwrap_or(b'0') - b'0') * 10)?;
    cents = cents.checked_add(i128::from(digits.next().unwrap_or(b'0') - b'0'))?;
    if negative {
        cents.checked_neg()
    } else {
        Some(cents)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DayGroup {
    pub date: Option<NaiveDate>,
    pub indices: Vec<usize>,
    pub summary: PeriodSummary,
}

#[must_use]
pub fn day_groups(transactions: &[Transaction]) -> Vec<DayGroup> {
    let mut days: BTreeMap<Option<NaiveDate>, Vec<usize>> = BTreeMap::new();
    for (index, transaction) in transactions.iter().enumerate() {
        days.entry(transaction_date(transaction))
            .or_default()
            .push(index);
    }
    days.into_iter()
        .rev()
        .map(|(date, indices)| {
            let summary = summarize_iter(indices.iter().map(|index| &transactions[*index]));
            DayGroup {
                date,
                indices,
                summary,
            }
        })
        .collect()
}

#[must_use]
pub fn money_cents(value: i128, currency: &str) -> String {
    let unsigned = value.unsigned_abs();
    let whole = (unsigned / 100).to_string();
    let mut grouped = String::new();
    for (index, digit) in whole.chars().enumerate() {
        if index > 0 && (whole.len() - index).is_multiple_of(3) {
            grouped.push('.');
        }
        grouped.push(digit);
    }
    let sign = if value < 0 { "-" } else { "" };
    format!("{sign}{grouped},{:02} {currency}", unsigned % 100)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(value: &str) -> NaiveDate {
        parse_date(value).unwrap()
    }

    fn transaction(
        id: &str,
        amount: &str,
        movement: &str,
        currency: Option<&str>,
        at: &str,
    ) -> Transaction {
        serde_json::from_value(serde_json::json!({
            "id": id, "account_id": "a", "amount": amount, "type": movement,
            "currency_code": currency, "transacted_at": at, "category_id": "food"
        }))
        .unwrap()
    }

    #[test]
    fn ranges_accept_both_date_formats_and_reject_invalid_or_reversed_dates() {
        let range = DateRange::parse("29/02/2024", "2024-03-01").unwrap();
        assert_eq!(range.query_start(), "2024-02-29");
        assert_eq!(range.query_end(), "2024-03-01");
        assert!(range.contains(date("2024-02-29")));
        assert!(range.contains(date("2024-03-01")));
        assert!(!range.contains(date("2024-03-02")));
        assert_eq!(
            DateRange::parse("2024-03-01", "29/02/2024"),
            Err(DateError::ReversedRange)
        );
        for invalid in [
            "29/02/2023",
            "2024-13-01",
            "2024-01-32",
            "1/1/2024",
            "2024-01-01oops",
            "",
        ] {
            assert_eq!(
                parse_date(invalid),
                Err(DateError::InvalidDate),
                "{invalid}"
            );
        }
    }

    #[test]
    fn periods_follow_calendar_boundaries_and_leap_years() {
        for (anchor, end) in [
            ("2024-02-15", "2024-02-29"),
            ("2023-02-28", "2023-02-28"),
            ("2100-02-05", "2100-02-28"),
            ("2000-02-05", "2000-02-29"),
        ] {
            assert_eq!(Period::month(date(anchor)).range().end(), date(end));
        }
        assert_eq!(
            Period::week(date("2024-01-01")).range(),
            DateRange::parse("2024-01-01", "2024-01-07").unwrap()
        );
        assert_eq!(
            Period::week(date("2023-01-01")).range(),
            DateRange::parse("2022-12-26", "2023-01-01").unwrap()
        );
        assert_eq!(
            Period::quarter(date("2026-12-29")).range(),
            DateRange::parse("2026-10-01", "2026-12-31").unwrap()
        );
        assert_eq!(
            Period::quarter(date("2026-04-01")).range(),
            DateRange::parse("2026-04-01", "2026-06-30").unwrap()
        );
        assert_eq!(
            Period::year(date("2024-02-29")).range(),
            DateRange::parse("2024-01-01", "2024-12-31").unwrap()
        );
    }

    #[test]
    fn navigation_always_selects_month_relative_to_range_start() {
        let december = Period::month(date("2025-12-31"));
        assert_eq!(
            december.next_month().range(),
            DateRange::parse("2026-01-01", "2026-01-31").unwrap()
        );
        assert_eq!(december.next_month().previous_month(), december);
        let custom = Period::custom(DateRange::parse("2024-03-31", "2024-06-01").unwrap());
        assert_eq!(
            custom.previous_month().range(),
            DateRange::parse("2024-02-01", "2024-02-29").unwrap()
        );
        assert_eq!(
            custom.next_month().range(),
            DateRange::parse("2024-04-01", "2024-04-30").unwrap()
        );
        assert_eq!(
            Period::current_month(date("2026-10-22")).label(),
            "Outubro 2026"
        );
    }

    #[test]
    fn timestamps_use_brasilia_day_and_date_only_is_not_shifted() {
        for (at, expected) in [
            ("2026-10-01T01:30:00Z", Some("2026-09-30")),
            ("2026-10-01T00:30:00-03:00", Some("2026-10-01")),
            ("2026-10-01T23:30:00-07:00", Some("2026-10-02")),
            ("2026-10-01", Some("2026-10-01")),
            ("2026-02-30", None),
            ("", None),
        ] {
            let item = transaction("t", "1.00", "credit", Some("BRL"), at);
            assert_eq!(transaction_date(&item), expected.map(date), "{at}");
        }
    }

    #[test]
    fn summaries_keep_exact_cents_and_separate_currencies_and_categories() {
        let mut rows = vec![
            transaction("a", "0.10", "credit", Some("BRL"), "2026-10-01"),
            transaction("b", "0.20", "credit", Some("BRL"), "2026-10-01"),
            transaction("c", "-0.15", "debit", Some("BRL"), "2026-10-01"),
            transaction("d", "2.0000", "debit", Some("USD"), "2026-10-01"),
            transaction("e", "4.00", "debit", None, "2026-10-01"),
            transaction("f", "20.00", "transfer", Some("BRL"), "2026-10-01"),
        ];
        rows[4].category_id = None;
        let mut ignored = rows[0].clone();
        ignored.ignored = true;
        rows.push(ignored);
        let totals = summarize(&rows);
        assert_eq!(
            totals.currencies["BRL"],
            FlowTotals {
                income: 30,
                expenses: 15,
                count: 3
            }
        );
        assert_eq!(totals.currencies["BRL"].result(), 15);
        assert_eq!(totals.currencies["USD"].expenses, 200);
        assert_eq!(totals.categories[""]["?"].expenses, 400);
        assert_eq!(totals.invalid_amounts, 0);
        assert_eq!(money_cents(1_234_567, "BRL"), "12.345,67 BRL");
        assert_eq!(money_cents(-123, "USD"), "-1,23 USD");
    }

    #[test]
    fn invalid_decimal_precision_and_overflow_are_counted_without_rounding() {
        let rows: Vec<_> = [
            "NaN",
            "inf",
            "1.001",
            "1.2.3",
            "",
            ".50",
            "100000000000000000000000000000000000000",
        ]
        .into_iter()
        .map(|amount| transaction("t", amount, "credit", Some("BRL"), "2026-10-01"))
        .collect();
        let totals = summarize(&rows);
        assert_eq!(totals.invalid_amounts, rows.len());
        assert!(totals.currencies.is_empty());
        let huge = "1701411834604692317316873037158841057.00";
        let rows = [
            transaction("a", huge, "credit", Some("BRL"), ""),
            transaction("b", "100.00", "credit", Some("BRL"), ""),
        ];
        let totals = summarize(&rows);
        assert_eq!(totals.invalid_amounts, 1);
        assert_eq!(totals.currencies["BRL"].count, 1);
    }

    #[test]
    fn day_groups_sort_descending_keep_order_and_keep_undated_rows() {
        let rows = [
            transaction("a", "10", "credit", Some("BRL"), "2026-10-01"),
            transaction("b", "2", "debit", Some("BRL"), "2026-10-02"),
            transaction("c", "1", "debit", Some("BRL"), "2026-10-01"),
            transaction("d", "3", "debit", Some("BRL"), "invalid"),
        ];
        let groups = day_groups(&rows);
        assert_eq!(
            groups.iter().map(|group| group.date).collect::<Vec<_>>(),
            vec![Some(date("2026-10-02")), Some(date("2026-10-01")), None]
        );
        assert_eq!(groups[1].indices, [0, 2]);
        assert_eq!(groups[1].summary.currencies["BRL"].result(), 900);
        assert_eq!(groups[2].indices, [3]);
    }
}
