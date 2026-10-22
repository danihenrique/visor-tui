use super::*;
use crate::{
    api::{ApiError, ApiErrorData},
    auth::Credential,
};
use hqtui::{buffer::FrameBuffer, input::InputParser};
use model::{format_amount, parse_amount, parse_date};
use serde_json::json;
use std::{cell::RefCell, rc::Rc};

fn transaction() -> Transaction {
    serde_json::from_value(json!({"id":"tx-1","account_id":"a-1","category_id":"c-1", "tag_ids":["t-1"],
        "amount":"12.34","type":"debit","description":"Mercado","transacted_at":"2026-10-22T09:14:00-03:00","source":"manual","currency_code":"BRL"})).unwrap()
}

fn screen() -> TransactionsScreen {
    TransactionsScreen {
        accounts: vec![serde_json::from_value(json!({"id":"a-1","name":"Principal"})).unwrap()],
        categories: vec![serde_json::from_value(json!({"id":"c-1","name":"Alimentação"})).unwrap()],
        tags: vec![
            serde_json::from_value(json!({"id":"t-1","name":"Casa"})).unwrap(),
            serde_json::from_value(json!({"id":"t-2","name":"Viagem"})).unwrap(),
        ],
        transactions: vec![transaction()],
        selected_index: Some(0),
        ..TransactionsScreen::new()
    }
}

fn keys(state: &mut TransactionsScreen, input: &str) -> Vec<Effect> {
    InputParser::new()
        .parse(input)
        .iter()
        .filter_map(|event| {
            if let InputEvent::Key(key) = event {
                Some(state.reduce_key(key))
            } else {
                None
            }
        })
        .collect()
}

fn valid_form() -> TransactionForm {
    let mut form = TransactionForm::new();
    form.amount = "1.234,56".into();
    form.description = "Pagamento".into();
    form.account_id = Some("a-1".into());
    form.category_id = Some("c-1".into());
    form.date = "22/10/2026".into();
    form
}

#[test]
fn refresh_button_bypasses_a_fresh_cache() {
    let mut server = mockito::Server::new();
    let read = server
        .mock("GET", "/v1/spaces/s/transactions")
        .match_query(mockito::Matcher::UrlEncoded("limit".into(), "100".into()))
        .with_body(r#"{"data":[],"next_cursor":null}"#)
        .expect(2)
        .create();
    let metadata: Vec<_> = ["accounts", "categories", "tags"]
        .into_iter()
        .map(|path| {
            server
                .mock("GET", format!("/v1/spaces/s/{path}").as_str())
                .with_body(r#"{"data":[]}"#)
                .expect(1)
                .create()
        })
        .collect();
    let api = ApiClient::for_test(
        &server.url(),
        Credential::PersonalToken {
            token: "visor_pat_mock".into(),
        },
        "s",
    )
    .unwrap();
    let mut state = screen();
    state.fetch_transactions(&api);
    let effect = state.click(view::Action::Key("r".into()));
    state.execute(effect, &api);
    assert!(state.reload_requested);
    state.load_initial(&api);
    read.assert();
    for request in metadata {
        request.assert();
    }
}

#[test]
fn manual_pagination_keys_never_fetch() {
    let mut state = screen();
    assert_eq!(keys(&mut state, "\u{1b}[C"), [Effect::None]);
    assert_eq!(keys(&mut state, "m"), [Effect::None]);
}

fn client(server: &mockito::Server) -> ApiClient {
    ApiClient::for_test(
        &server.url(),
        Credential::PersonalToken {
            token: "mock-token".into(),
        },
        "s",
    )
    .unwrap()
}

#[test]
fn amounts_are_exact_positive_decimal_strings() {
    for (input, expected) in [
        ("1.234,56", "1234.56"),
        ("R$ 0,01", "0.01"),
        ("9999999999999.99", "9999999999999.99"),
        ("00001,2", "1.20"),
    ] {
        assert_eq!(parse_amount(input), Ok(expected.into()));
    }
    for input in [
        "0",
        "-5",
        "+5",
        "NaN",
        "inf",
        "1e4",
        "2.345",
        "1.2,30",
        "99999999999999",
        "1,2,3",
    ] {
        assert!(parse_amount(input).is_err(), "accepted {input}");
    }
    assert_eq!(
        format_amount("9999999999999.99", "CREDIT"),
        "+R$ 9.999.999.999.999,99"
    );
    assert_eq!(format_amount("0", "debit"), "-R$ 0,00");
}

#[test]
fn dates_validate_the_calendar_and_normalize_local_format() {
    assert_eq!(parse_date("29/02/2024").unwrap().to_string(), "2024-02-29");
    for invalid in ["2026-02-29", "2026-13-01", "2026-1-2", "31/04/2026", ""] {
        assert!(parse_date(invalid).is_err());
    }
}

#[test]
fn required_fields_block_submit_and_focus_first_error() {
    let mut form = TransactionForm::new();
    assert!(form.payload().is_none());
    assert_eq!(form.focused, FormField::Amount);
    for field in [
        FormField::Amount,
        FormField::Description,
        FormField::Account,
        FormField::Category,
    ] {
        assert!(form.errors.contains_key(&field));
    }
    let mut form = valid_form();
    let body = form.payload().unwrap();
    assert_eq!(body["amount"], "1234.56");
    assert_eq!(body["type"], "debit");
    assert!(chrono::DateTime::parse_from_rfc3339(body["transacted_at"].as_str().unwrap()).is_ok());
}

#[test]
fn payload_checks_api_lengths_and_tag_limit() {
    let mut form = valid_form();
    form.description = "a".repeat(256);
    form.notes = "b".repeat(501);
    form.merchant = "c".repeat(256);
    form.tag_ids = (0..21).map(|i| format!("tag-{i}")).collect();
    assert!(form.payload().is_none());
    for field in [
        FormField::Description,
        FormField::Notes,
        FormField::Merchant,
        FormField::Tags,
    ] {
        assert!(form.errors.contains_key(&field));
    }
}

#[test]
fn edit_preserves_tags_and_timestamp_and_can_clear_tags() {
    let mut form = TransactionForm::edit(transaction());
    assert_eq!(form.tag_ids, ["t-1"]);
    let body = form.payload().unwrap();
    assert_eq!(body["tag_ids"], json!(["t-1"]));
    assert!(body.get("transacted_at").is_none());
    assert!(body.get("account_id").is_none());
    assert!(body.get("merchant_name").is_none());
    form.tag_ids.clear();
    assert_eq!(form.payload().unwrap()["tag_ids"], json!([]));
}

#[test]
fn unchanged_edit_text_is_omitted_so_whitespace_is_preserved() {
    let mut tx = transaction();
    tx.description = Some("  Mercado  ".into());
    tx.notes = Some("   ".into());
    let mut form = TransactionForm::edit(tx);
    let body = form.payload().unwrap();
    assert!(body.get("description").is_none());
    assert!(body.get("notes").is_none());
    form.notes = "nova nota".into();
    assert_eq!(form.payload().unwrap()["notes"], "nova nota");
    form.notes.clear();
    assert_eq!(form.payload().unwrap()["notes"], "");
}

#[test]
fn imported_transactions_only_send_editable_fields() {
    for source in ["connected", "apple_pay"] {
        let mut tx = transaction();
        tx.source = Some(source.into());
        let mut form = TransactionForm::edit(tx);
        form.amount = "broken".into();
        form.date = "broken".into();
        form.description.clear();
        let body = form.payload().unwrap();
        for field in [
            "amount",
            "description",
            "type",
            "account_id",
            "merchant_name",
            "transacted_at",
        ] {
            assert!(body.get(field).is_none(), "sent immutable {field}");
        }
        assert_eq!(form.focused, FormField::Category);
        form.move_focus(true);
        assert_eq!(form.focused, FormField::Ignored);
        form.move_focus(false);
        assert_eq!(form.focused, FormField::Category);
    }
}

#[test]
fn real_terminal_keys_cycle_focus_without_leaking_shortcuts() {
    let mut state = screen();
    keys(&mut state, "n");
    assert!(state.is_modal_open());
    keys(&mut state, "12,34\tq2/r");
    let TransactionMode::Form(form) = &state.mode else {
        panic!("form closed");
    };
    assert_eq!(form.amount, "12,34");
    assert_eq!(form.description, "q2/r");
    keys(&mut state, "\x1b[Z");
    let TransactionMode::Form(form) = &state.mode else {
        panic!("form closed");
    };
    assert_eq!(form.focused, FormField::Amount);
    keys(&mut state, "\x15");
    let TransactionMode::Form(form) = &state.mode else {
        panic!("form closed");
    };
    assert_eq!(form.amount, "");
}

#[test]
fn edit_and_export_have_distinct_bindings() {
    let mut state = screen();
    assert_eq!(keys(&mut state, "e"), [Effect::None]);
    assert!(matches!(state.mode, TransactionMode::Form(_)));
    state.mode = TransactionMode::List;
    assert_eq!(keys(&mut state, "E"), [Effect::Export]);
    assert!(matches!(state.mode, TransactionMode::List));
    assert_eq!(keys(&mut state, "\x05"), [Effect::Export]);
}

#[test]
fn picker_filters_by_name_selects_by_id_and_toggles_existing_tags() {
    let mut state = screen();
    state.mode = TransactionMode::Form(Box::new(TransactionForm::edit(transaction())));
    if let TransactionMode::Form(form) = &mut state.mode {
        form.focused = FormField::Tags;
    }
    keys(&mut state, "\rCasa\r");
    let TransactionMode::Form(form) = &state.mode else {
        panic!()
    };
    assert_eq!(form.tag_ids, Vec::<String>::new());
    assert!(form.picker.is_some());
    keys(&mut state, "\x15Viagem\r");
    let TransactionMode::Form(form) = &state.mode else {
        panic!()
    };
    assert_eq!(form.tag_ids, ["t-2"]);
}

#[test]
fn filters_stage_changes_and_clear_all_dimensions_without_changing_period() {
    let mut state = screen();
    let period = state.period;
    keys(&mut state, "f");
    assert!(matches!(&state.mode, TransactionMode::Filters(_)));
    assert_eq!(state.filters, TransactionFilters::default());
    keys(&mut state, "\x13");
    assert!(!state.is_modal_open());
    assert_eq!(state.period, period);
    state.filters.account_id = Some("a-1".into());
    state.filters.category_id = Some("c-1".into());
    state.filters.tag_id = Some("t-1".into());
    state.filters.tx_type = TxTypeFilter::Debit;
    state.filters.ignored = IgnoredFilter::OnlyIgnored;
    state.filters.search = "mercado".into();
    assert_eq!(keys(&mut state, "c"), [Effect::Fetch]);
    assert_eq!(state.filters, TransactionFilters::default());
}

#[test]
fn filter_picker_can_return_to_all_accounts() {
    let mut state = screen();
    state.filters.account_id = Some("a-1".into());
    keys(&mut state, "f\r\r\x13");
    assert_eq!(state.filters.account_id, None);
}

#[test]
fn delete_requires_explicit_confirmation_and_selection_moves_safely() {
    let mut state = screen();
    keys(&mut state, "kkkkjjjjd");
    assert_eq!(keys(&mut state, "\r"), [Effect::None]);
    assert!(!state.is_modal_open());
    keys(&mut state, "d\t");
    assert_eq!(keys(&mut state, "\r"), [Effect::Delete("tx-1".into())]);
    state.mode = TransactionMode::List;
    state.transactions.clear();
    keys(&mut state, "jkd");
    assert_eq!(state.selected_index, None);
    assert!(!state.is_modal_open());
}

#[test]
fn server_validation_errors_are_attached_to_fields() {
    let mut form = valid_form();
    form.api_error(&ApiError::Api(Box::new(ApiErrorData { status: 422, code: "VALIDATION_ERROR".into(), message: "Invalid".into(),
        details: json!({"issues":[{"path":["amount"],"message":"Limite excedido"}],"field_errors":{"category_id":["Categoria indisponível"]}}), request_id: None, display: "Invalid".into() })));
    assert_eq!(form.errors[&FormField::Amount], "Limite excedido");
    assert_eq!(form.errors[&FormField::Category], "Categoria indisponível");
    assert_eq!(form.focused, FormField::Amount);
}

#[test]
fn create_uses_normalized_amount_lowercase_type_and_required_category() {
    let mut server = mockito::Server::new();
    let api = client(&server);
    let create = server.mock("POST", "/v1/spaces/s/transactions")
        .match_body(mockito::Matcher::PartialJson(json!({"amount":"1234.56","description":"Pagamento","category_id":"c-1","account_id":"a-1","type":"debit"})))
        .with_status(201).with_body(json!({"data":transaction()}).to_string()).create();
    let refresh = server
        .mock("GET", "/v1/spaces/s/transactions")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_body(json!({"data":[transaction()]}).to_string())
        .create();
    let mut state = screen();
    state.mode = TransactionMode::Form(Box::new(valid_form()));
    let effect = keys(&mut state, "\x13").pop().unwrap();
    state.execute(effect, &api);
    assert!(!state.is_modal_open());
    create.assert();
    assert!(state.reload_requested);
    state.fetch_transactions(&api);
    refresh.assert();
}

#[test]
fn uncertain_create_retries_same_body_and_key_without_allowing_edits() {
    let mut server = mockito::Server::new();
    let api = client(&server);
    let form = valid_form();
    let key = form.create_key.clone();
    let first = server
        .mock("POST", "/v1/spaces/s/transactions")
        .match_header("idempotency-key", key.as_str())
        .with_status(201)
        .with_body("incomplete response")
        .expect(1)
        .create();
    let replay = server
        .mock("POST", "/v1/spaces/s/transactions")
        .match_header("idempotency-key", key.as_str())
        .with_status(201)
        .with_body(json!({"data":transaction()}).to_string())
        .expect(1)
        .create();
    let refresh = server
        .mock("GET", "/v1/spaces/s/transactions")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_body(json!({"data":[]}).to_string())
        .create();
    let mut state = screen();
    state.mode = TransactionMode::Form(Box::new(form));
    let first_effect = keys(&mut state, "\x13").pop().unwrap();
    let first_body = match &first_effect {
        Effect::Save(body) => body.clone(),
        _ => panic!("expected save"),
    };
    state.execute(first_effect, &api);
    let TransactionMode::Form(form) = &state.mode else {
        panic!("draft closed after uncertain response")
    };
    assert_eq!(form.pending_create.as_ref(), Some(&first_body));
    assert_eq!(form.create_key, key);
    keys(&mut state, "\tXYZ");
    if let TransactionMode::Form(form) = &state.mode {
        assert_eq!(form.description, "Pagamento");
    }
    let retry = keys(&mut state, "\x13").pop().unwrap();
    assert_eq!(retry, Effect::Save(first_body));
    state.execute(retry, &api);
    assert!(!state.is_modal_open());
    first.assert();
    replay.assert();
    assert!(state.reload_requested);
    state.fetch_transactions(&api);
    refresh.assert();
}

#[test]
fn rejected_create_unlocks_draft_and_uses_a_new_key_after_correction() {
    let mut server = mockito::Server::new();
    let api = client(&server);
    let rejected = server
        .mock("POST", "/v1/spaces/s/transactions")
        .with_status(422)
        .with_body(json!({"errors":[{"code":"VALIDATION_ERROR","message":"Invalid"}]}).to_string())
        .expect(1)
        .create();
    let mut state = screen();
    let form = valid_form();
    let original_key = form.create_key.clone();
    state.mode = TransactionMode::Form(Box::new(form));
    let effect = keys(&mut state, "\x13").pop().unwrap();
    state.execute(effect, &api);
    let TransactionMode::Form(form) = &state.mode else {
        panic!("rejected draft closed")
    };
    assert!(form.pending_create.is_none());
    assert_ne!(form.create_key, original_key);
    let corrected_key = form.create_key.clone();
    keys(&mut state, "\tX");
    let TransactionMode::Form(form) = &state.mode else {
        panic!()
    };
    assert_eq!(form.description, "PagamentoX");
    let accepted = server
        .mock("POST", "/v1/spaces/s/transactions")
        .match_header("idempotency-key", corrected_key.as_str())
        .match_body(mockito::Matcher::PartialJson(
            json!({"description":"PagamentoX"}),
        ))
        .with_status(201)
        .with_body(json!({"data":transaction()}).to_string())
        .expect(1)
        .create();
    let refresh = server
        .mock("GET", "/v1/spaces/s/transactions")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_body(json!({"data":[]}).to_string())
        .create();
    let effect = keys(&mut state, "\x13").pop().unwrap();
    state.execute(effect, &api);
    assert!(!state.is_modal_open());
    rejected.assert();
    accepted.assert();
    assert!(state.reload_requested);
    state.fetch_transactions(&api);
    refresh.assert();
}

#[test]
fn connected_edit_patch_omits_immutable_fields_and_clears_tags() {
    let mut server = mockito::Server::new();
    let api = client(&server);
    let mut tx = transaction();
    tx.source = Some("connected".into());
    let patch = server
        .mock("PATCH", "/v1/spaces/s/transactions/tx-1")
        .match_body(mockito::Matcher::Json(
            json!({"category_id":"c-1","tag_ids":[],"ignored":true}),
        ))
        .with_status(200)
        .with_body(json!({"data":tx}).to_string())
        .create();
    let refresh = server
        .mock("GET", "/v1/spaces/s/transactions")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_body(json!({"data":[]}).to_string())
        .create();
    let mut form = TransactionForm::edit(tx);
    form.tag_ids.clear();
    form.ignored = true;
    let mut state = screen();
    state.mode = TransactionMode::Form(Box::new(form));
    let effect = keys(&mut state, "\x13").pop().unwrap();
    state.execute(effect, &api);
    assert!(!state.is_modal_open());
    patch.assert();
    assert!(state.reload_requested);
    state.fetch_transactions(&api);
    refresh.assert();
}

#[test]
fn filter_submission_fetches_all_dimensions_once() {
    let mut server = mockito::Server::new();
    let api = client(&server);
    let request = server
        .mock("GET", "/v1/spaces/s/transactions")
        .match_query(mockito::Matcher::AllOf(vec![
            mockito::Matcher::UrlEncoded("from_date".into(), "2026-10-01".into()),
            mockito::Matcher::UrlEncoded("to_date".into(), "2026-10-22".into()),
            mockito::Matcher::UrlEncoded("account_id".into(), "a-1".into()),
            mockito::Matcher::UrlEncoded("category_id".into(), "c-1".into()),
            mockito::Matcher::UrlEncoded("tag_id".into(), "t-1".into()),
            mockito::Matcher::UrlEncoded("type".into(), "debit".into()),
            mockito::Matcher::UrlEncoded("ignored".into(), "true".into()),
            mockito::Matcher::UrlEncoded("search".into(), "café & pão".into()),
        ]))
        .with_status(200)
        .with_body(json!({"data":[]}).to_string())
        .expect(1)
        .create();
    let mut state = screen();
    state.period =
        Period::custom(crate::period::DateRange::parse("2026-10-01", "2026-10-22").unwrap());
    state.mode = TransactionMode::Filters(FilterForm::new(TransactionFilters {
        account_id: Some("a-1".into()),
        category_id: Some("c-1".into()),
        tag_id: Some("t-1".into()),
        tx_type: TxTypeFilter::Debit,
        ignored: IgnoredFilter::OnlyIgnored,
        search: "café & pão".into(),
    }));
    let effect = keys(&mut state, "\x13").pop().unwrap();
    state.execute(effect, &api);
    assert!(state.reload_requested);
    state.fetch_transactions(&api);
    request.assert();
}

#[test]
fn rendering_keeps_focused_field_and_controls_visible_at_small_sizes() {
    for (width, height) in [(120, 40), (80, 24), (60, 18), (40, 12)] {
        let state = screen();
        let mut state = TransactionsScreen {
            mode: TransactionMode::Form(Box::new(valid_form())),
            ..state
        };
        if let TransactionMode::Form(form) = &mut state.mode {
            form.focused = FormField::Ignored;
        }
        let buffer = Rc::new(RefCell::new(FrameBuffer::new(width, height)));
        let surface = Surface::root(buffer.clone(), Rc::new(hqtui::theme::dark()));
        view::draw(&surface, &state);
        let text: String = buffer
            .borrow()
            .chars
            .iter()
            .filter_map(|cell| char::from_u32(*cell))
            .collect();
        assert!(
            text.contains("Ignorado"),
            "focused field hidden at {width}x{height}"
        );
        assert!(
            text.contains("Salvar"),
            "save action hidden at {width}x{height}"
        );
        assert!((0..height).any(|y| (0..width).any(|x| state.hitboxes.at(x, y).is_some())));
    }
}

#[test]
fn transaction_dialogs_preserve_original_currency() {
    for currency in [Some("USD"), None] {
        let mut tx = transaction();
        tx.currency_code = currency.map(str::to_owned);
        let code = currency.unwrap_or("?");
        for mode in [
            TransactionMode::Detail(tx.clone()),
            TransactionMode::DeleteConfirm {
                transaction: tx.clone(),
                confirmed: false,
            },
            TransactionMode::Form(Box::new(TransactionForm::edit(tx.clone()))),
        ] {
            let editing = matches!(mode, TransactionMode::Form(_));
            let state = TransactionsScreen {
                transactions: vec![tx.clone()],
                mode,
                ..screen()
            };
            let buffer = Rc::new(RefCell::new(FrameBuffer::new(80, 24)));
            let surface = Surface::root(buffer.clone(), Rc::new(hqtui::theme::dark()));
            view::draw(&surface, &state);
            let text: String = buffer
                .borrow()
                .chars
                .iter()
                .filter_map(|cell| char::from_u32(*cell))
                .collect();
            let expected = if editing {
                format!("Valor ({code})")
            } else {
                format!("-{code} 12,34")
            };
            assert!(
                text.contains(&expected),
                "Expected original currency {expected:?}, got:\n{text}"
            );
            assert!(
                !text.contains("R$"),
                "A foreign or unknown amount was labeled BRL:\n{text}"
            );
        }
    }
}

#[test]
fn new_transaction_amount_label_uses_selected_account_currency() {
    let mut state = screen();
    state.accounts[0].currency_code = Some("EUR".into());
    state.mode = TransactionMode::Form(Box::new(valid_form()));
    let buffer = Rc::new(RefCell::new(FrameBuffer::new(80, 24)));
    let surface = Surface::root(buffer.clone(), Rc::new(hqtui::theme::dark()));
    view::draw(&surface, &state);
    let text: String = buffer
        .borrow()
        .chars
        .iter()
        .filter_map(|cell| char::from_u32(*cell))
        .collect();
    assert!(
        text.contains("Valor (EUR)"),
        "Expected selected account currency, got:\n{text}"
    );
}

#[test]
fn table_amounts_fit_without_a_trailing_ellipsis() {
    for width in [120, 100, 80, 60, 40] {
        let state = screen();
        let buffer = Rc::new(RefCell::new(FrameBuffer::new(width, 24)));
        let surface = Surface::root(buffer.clone(), Rc::new(hqtui::theme::dark()));
        view::draw(&surface, &state);
        let data = buffer.borrow();
        let row: String = data
            .chars
            .chunks(width)
            .find_map(|cells| {
                let row: String = cells
                    .iter()
                    .filter_map(|cell| char::from_u32(*cell))
                    .collect();
                row.contains("R$ 12,34").then_some(row)
            })
            .expect("amount missing");
        assert!(
            row.trim_end_matches([' ', '│']).ends_with("R$ 12,34"),
            "amount clipped at {width}: {row}"
        );
    }
}

#[test]
fn active_filter_chips_wrap_and_keep_all_dimensions_visible() {
    let mut state = screen();
    state.filters = TransactionFilters {
        account_id: Some("a-1".into()),
        category_id: Some("c-1".into()),
        tag_id: Some("t-1".into()),
        tx_type: TxTypeFilter::Debit,
        ignored: IgnoredFilter::OnlyIgnored,
        search: "café".into(),
    };
    let buffer = Rc::new(RefCell::new(FrameBuffer::new(80, 24)));
    let surface = Surface::root(buffer.clone(), Rc::new(hqtui::theme::dark()));
    view::draw(&surface, &state);
    let text: String = buffer
        .borrow()
        .chars
        .iter()
        .filter_map(|cell| char::from_u32(*cell))
        .collect();
    for chip in [
        "Busca:",
        "Conta:",
        "Categoria:",
        "Tag:",
        "Despesa",
        "Ignorados",
    ] {
        assert!(text.contains(chip), "missing chip {chip}");
    }
}

fn transaction_lines(state: &TransactionsScreen, width: usize, height: usize) -> Vec<String> {
    let buffer = Rc::new(RefCell::new(FrameBuffer::new(width, height)));
    let surface = Surface::root(buffer.clone(), Rc::new(hqtui::theme::dark()));
    view::draw(&surface, state);
    buffer
        .borrow()
        .chars
        .chunks(width)
        .map(|row| {
            row.iter()
                .filter_map(|cell| char::from_u32(*cell))
                .collect()
        })
        .collect()
}

#[test]
fn loaded_totals_and_category_spending_exclude_ignored_and_income() {
    let mut state = screen();
    let mut expense = transaction();
    expense.amount = "30.00".into();
    let mut other = expense.clone();
    other.category_id = None;
    other.amount = "10.00".into();
    let mut income = expense.clone();
    income.tx_type = "CREDIT".into();
    income.amount = "200.00".into();
    let mut ignored = expense.clone();
    ignored.ignored = true;
    ignored.amount = "900.00".into();
    state.transactions = vec![expense, other, income, ignored];
    let lines = transaction_lines(&state, 120, 40);
    let summary = lines[..6].join("\n");
    assert!(summary.contains("4 lançamentos"));
    assert!(summary.contains("200,00 BRL"));
    assert!(summary.contains("40,00 BRL"));
    assert!(summary.contains("160,00 BRL"));
    assert!(!summary.contains("Parcial"));
    assert!(summary.contains("sem ignorados"));
    let grouped = lines.join("\n");
    assert!(grouped.contains("22/10/2026"));
    assert!(grouped.contains("R 200,00 BRL  D 40,00 BRL  Saldo 160,00 BRL"));
}

#[test]
fn transactions_keep_separate_original_currency_totals() {
    let mut state = screen();
    let mut foreign = transaction();
    foreign.amount = "5.00".into();
    foreign.currency_code = Some("USD".into());
    state.transactions.push(foreign);
    let lines = transaction_lines(&state, 120, 40);
    assert!(lines[..6].join("\n").contains("USD original"));
    assert!(lines[..6].join("\n").contains("12,34 BRL"));
    let rendered = lines.join("\n");
    assert!(rendered.contains("-USD 5,00"));
    assert!(!rendered.contains("Gastos por categoria"));
}

#[test]
fn selected_transaction_has_a_marker_highlight_and_click_target() {
    for (width, height) in [(95, 36), (78, 20), (58, 14)] {
        let state = screen();
        let buffer = Rc::new(RefCell::new(FrameBuffer::new(width, height)));
        let theme = Rc::new(hqtui::theme::dark());
        let surface = Surface::root(buffer.clone(), theme.clone());
        view::draw(&surface, &state);
        let buffer = buffer.borrow();
        let selected = buffer
            .chars
            .iter()
            .position(|cell| *cell == u32::from('▸'))
            .expect("selection marker");
        let y = selected / width;
        assert_eq!(buffer.bg[selected + 1], theme.selection);
        assert!(matches!(
            state.hitboxes.at(3, y),
            Some(view::Action::Row(0))
        ));
        if width >= 65 {
            assert!(buffer.chars.chunks(width).any(|cells| {
                cells
                    .iter()
                    .filter_map(|cell| char::from_u32(*cell))
                    .collect::<String>()
                    .contains("CATEGORIA")
            }));
        }
    }
}

#[test]
fn errors_and_loading_never_present_an_empty_period_as_complete() {
    let mut state = screen();
    state.transactions.clear();
    state.error = Some("Página indisponível".into());
    let text = transaction_lines(&state, 93, 20).join("\n");
    assert!(text.contains("Totais indisponíveis"));
    assert!(!text.contains("0,00 BRL"));
    assert!(!text.contains("0 lançamentos"));
    state.error = None;
    state.loading = true;
    let text = transaction_lines(&state, 93, 20).join("\n");
    assert!(text.contains("Carregando período completo"));
    assert!(!text.contains("0,00 BRL"));
}

#[test]
fn daily_headers_do_not_change_transaction_selection_or_mouse_targets() {
    let mut state = screen();
    let mut second = transaction();
    second.id = "second".into();
    second.transacted_at = Some("2026-10-21".into());
    state.transactions.push(second);
    keys(&mut state, "j");
    let lines = transaction_lines(&state, 93, 20);
    assert!(lines.join("\n").contains("21/10/2026"));
    let hit = (0..20)
        .flat_map(|y| (0..93).map(move |x| (x, y)))
        .find_map(|(x, y)| {
            state
                .hitboxes
                .at(x, y)
                .filter(|action| matches!(action, view::Action::Row(1)))
        })
        .unwrap();
    state.click(hit);
    keys(&mut state, "e");
    assert!(
        matches!(&state.mode, TransactionMode::Form(form) if form.original.as_ref().unwrap().id == "second")
    );
}

#[test]
fn account_and_category_fit_actual_content_width_and_compact_totals_remain_visible() {
    let state = screen();
    let wide = transaction_lines(&state, 93, 36).join("\n");
    for value in [
        "CATEGORIA",
        "CONTA",
        "Alimentação",
        "Principal",
        "-R$ 12,34",
    ] {
        assert!(wide.contains(value), "missing {value}\n{wide}");
    }
    let compact = transaction_lines(&state, 80, 20).join("\n");
    for value in [
        "Receitas",
        "Despesas",
        "Saldo do período",
        "1 lançamentos",
        "Mercado",
    ] {
        assert!(compact.contains(value), "missing {value}\n{compact}");
    }
}

#[test]
fn mouse_row_double_click_opens_detail_and_modal_clicks_do_not_reach_the_list() {
    use crate::interaction::test_support::{click, click_text};
    let api = ApiClient::for_test(
        "http://127.0.0.1:1",
        Credential::PersonalToken {
            token: "test-only".into(),
        },
        "s",
    )
    .unwrap();
    let mut state = screen();
    let text = transaction_lines(&state, 80, 20).join("\n");
    let row = click_text(&text, "Mercado");
    state.handle_input(&api, std::slice::from_ref(&row));
    assert!(matches!(state.mode, TransactionMode::List));
    state.handle_input(&api, &[row]);
    assert!(matches!(state.mode, TransactionMode::Detail(_)));
    transaction_lines(&state, 80, 20);
    state.handle_input(&api, &[click(0, 0)]);
    assert!(matches!(state.mode, TransactionMode::Detail(_)));
    state.handle_input(&api, &[crate::interaction::key_event("escape")]);
    let text = transaction_lines(&state, 80, 17).join("\n");
    state.handle_input(&api, &[click_text(&text, "n Nova")]);
    let text = transaction_lines(&state, 80, 17).join("\n");
    let mut wheel = click_text(&text, "Valor");
    if let InputEvent::Mouse(mouse) = &mut wheel {
        mouse.action = MouseAction::Scroll;
        mouse.scroll = 1;
    }
    for _ in 0..4 {
        state.handle_input(&api, std::slice::from_ref(&wheel));
        transaction_lines(&state, 80, 17);
    }
    assert!(
        matches!(&state.mode, TransactionMode::Form(form) if form.focused == FormField::Category)
    );
}

#[test]
fn picker_back_button_returns_to_form_and_preserves_selected_tags() {
    use crate::interaction::test_support::click_text;
    let api = ApiClient::for_test(
        "http://127.0.0.1:1",
        Credential::PersonalToken {
            token: "test-only".into(),
        },
        "s",
    )
    .unwrap();
    for (width, height) in [(60, 18), (80, 24), (120, 40)] {
        for field in [FormField::Account, FormField::Category, FormField::Tags] {
            let mut state = screen();
            let mut form = valid_form();
            form.focused = field;
            state.mode = TransactionMode::Form(Box::new(form));
            keys(&mut state, "\r");
            assert!(state.picker().is_some());
            let text = transaction_lines(&state, width, height).join("\n");
            if field == FormField::Tags {
                state.handle_input(&api, &[click_text(&text, "Viagem")]);
                assert!(state.picker().is_some());
            }
            let text = transaction_lines(&state, width, height).join("\n");
            state.handle_input(&api, &[click_text(&text, "Esc Voltar")]);
            assert!(state.picker().is_none());
            let TransactionMode::Form(form) = &state.mode else {
                panic!("picker closed the form");
            };
            assert_eq!(form.focused, field);
            assert_eq!(form.amount, "1.234,56");
            if field == FormField::Tags {
                assert_eq!(form.tag_ids, ["t-2"]);
            }
        }
    }
}
