use super::*;
use crate::{
    api::{ApiError, ApiErrorData},
    auth::Credential,
};
use hqtui::{buffer::FrameBuffer, input::InputParser};
use model::{format_amount, parse_amount, parse_date};
use serde_json::json;

fn transaction() -> Transaction {
    serde_json::from_value(json!({"id":"tx-1","account_id":"a-1","category_id":"c-1", "tag_ids":["t-1"],
        "amount":"12.34","type":"debit","description":"Mercado","transacted_at":"2026-10-22T09:14:00-03:00","source":"manual"})).unwrap()
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
fn filters_stage_changes_validate_dates_and_clear_all_dimensions() {
    let mut state = screen();
    keys(&mut state, "f2026-10-23\t2026-10-01\x13");
    assert!(matches!(&state.mode, TransactionMode::Filters(form) if form.error.is_some()));
    assert_eq!(state.filters, TransactionFilters::default());
    keys(&mut state, "\x15\x13");
    assert_eq!(state.filters.date_from, "2026-10-23");
    assert!(!state.is_modal_open());
    state.filters.account_id = Some("a-1".into());
    state.filters.category_id = Some("c-1".into());
    state.filters.tag_id = Some("t-1".into());
    state.filters.tx_type = TxTypeFilter::Debit;
    state.filters.ignored = IgnoredFilter::OnlyIgnored;
    state.filters.search = "mercado".into();
    assert_eq!(keys(&mut state, "c"), [Effect::Fetch { append: false }]);
    assert_eq!(state.filters, TransactionFilters::default());
}

#[test]
fn filter_picker_can_return_to_all_accounts() {
    let mut state = screen();
    state.filters.account_id = Some("a-1".into());
    keys(&mut state, "f\t\t\r\r\x13");
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
            json!({"category_id":"c-1","tag_ids":[],"notes":"","ignored":true}),
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
    state.mode = TransactionMode::Filters(FilterForm::new(TransactionFilters {
        date_from: "01/10/2026".into(),
        date_to: "22/10/2026".into(),
        account_id: Some("a-1".into()),
        category_id: Some("c-1".into()),
        tag_id: Some("t-1".into()),
        tx_type: TxTypeFilter::Debit,
        ignored: IgnoredFilter::OnlyIgnored,
        search: "café & pão".into(),
    }));
    let effect = keys(&mut state, "\x13").pop().unwrap();
    state.execute(effect, &api);
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
        assert!(!state.hitboxes.borrow().is_empty());
    }
}
