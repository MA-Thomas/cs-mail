use cs_mail_correspondence::*;
use cs_mail_primitives::MessageId;
use serde_json::json;

fn image() -> ImageContent {
    ImageContent::new(
        ImageFormat::Png,
        include_bytes!("fixtures/pixel.png").to_vec(),
    )
    .unwrap()
}
fn document() -> (Document, MessageManifest) {
    let d = Document::new(
        vec![
            DocumentPart::Text("before café".into()),
            DocumentPart::Image(image()),
            DocumentPart::Text("after https://example.test".into()),
        ],
        [7; 32],
    )
    .unwrap();
    let m = d
        .manifest(MessageId(1), ConversationId::new(1).unwrap())
        .unwrap();
    (d, m)
}
fn selection(manifest: &MessageManifest, elements: Vec<SelectionElement>) -> MessageSelection {
    MessageSelection::new(manifest.message(), manifest.version(), elements).unwrap()
}

// Claim: hostile restored selections cannot reorder/duplicate content, switch a
// text range to an image, or split a Unicode character during endpoint extraction.
// Any accepted malformed selection would misrepresent what the user selected.
#[test]
fn hostile_selection_input_cannot_change_order_types_or_character_boundaries() {
    let (d, m) = document();
    for elements in [
        json!([]),
        json!([{"ImageBlock":{"block":1}}, {"TextRange":{"block":0,"start":0,"end":1}}]),
        json!([{"ImageBlock":{"block":1}}, {"ImageBlock":{"block":1}}]),
        json!([{"TextRange":{"block":0,"start":0,"end":4}}, {"TextRange":{"block":0,"start":3,"end":5}}]),
        json!([{"TextRange":{"block":0,"start":1,"end":1}}]),
        json!([{"TextRange":{"block":128,"start":0,"end":1}}]),
        json!([{"ImageBlock":{"block":1,"url":"https://example.test"}}]),
    ] {
        assert!(
            serde_json::from_value::<MessageSelection>(
                json!({"message":1,"version":m.version(),"elements":elements})
            )
            .is_err()
        );
    }
    for element in [
        SelectionElement::TextRange {
            block: 1,
            start: 0,
            end: 1,
        },
        SelectionElement::ImageBlock { block: 0 },
        SelectionElement::ImageBlock { block: 3 },
        SelectionElement::TextRange {
            block: 0,
            start: 0,
            end: 13,
        },
    ] {
        assert!(selection(&m, vec![element]).validate_target(&m).is_err());
    }
    // The server can check byte bounds, but only the endpoint can reject half of é.
    let split = selection(
        &m,
        vec![SelectionElement::TextRange {
            block: 0,
            start: 10,
            end: 11,
        }],
    );
    assert!(split.validate_target(&m).is_ok());
    assert!(d.select(&m, &split).is_err());
    let stale = MessageSelection::new(
        m.message(),
        MessageVersion([0; 32]),
        vec![SelectionElement::ImageBlock { block: 1 }],
    )
    .unwrap();
    assert!(stale.validate_target(&m).is_err());
}

// Claim: a compound quote preserves precisely the chosen ordered content, and a
// selection of that quote addresses its retained blocks rather than its wrapper.
// Wrong text/image extraction would falsify stable quotation and reference semantics.
#[test]
fn mixed_selection_and_selection_of_a_quote_preserve_exact_content() {
    let (d, m) = document();
    let s = selection(
        &m,
        vec![
            SelectionElement::TextRange {
                block: 0,
                start: 7,
                end: 12,
            },
            SelectionElement::ImageBlock { block: 1 },
            SelectionElement::TextRange {
                block: 2,
                start: 0,
                end: 5,
            },
        ],
    );
    let expected = vec![
        ContentBlock::Text("café".into()),
        ContentBlock::Image(image()),
        ContentBlock::Text("after".into()),
    ];
    assert_eq!(d.select(&m, &s).unwrap(), expected);
    let q = d.quote(&m, s.clone()).unwrap();
    assert_eq!(q.blocks(), expected);
    assert_eq!(q.source(), &s);
    let copy = Document::new(
        vec![
            DocumentPart::Reference(SourceReference(s)),
            DocumentPart::Quotation(q),
        ],
        [8; 32],
    )
    .unwrap();
    let cm = copy.manifest(MessageId(2), m.conversation()).unwrap();
    assert_eq!(cm.blocks()[0], BlockDescriptor::Reference);
    let nested = selection(&cm, vec![SelectionElement::ImageBlock { block: 2 }]);
    assert_eq!(
        copy.quote(&cm, nested).unwrap().blocks(),
        [ContentBlock::Image(image())]
    );
    assert!(
        selection(
            &cm,
            vec![SelectionElement::TextRange {
                block: 0,
                start: 0,
                end: 1
            }]
        )
        .validate_target(&cm)
        .is_err()
    );
    let mut restored = serde_json::to_value(&copy).unwrap();
    restored["parts"][1]["Quotation"]["blocks"][1] = json!({"Text":"substituted image"});
    assert!(serde_json::from_value::<Document>(restored).is_err());
}
