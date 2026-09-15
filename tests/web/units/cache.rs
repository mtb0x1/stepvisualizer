use std::cell::Cell;
use std::rc::Rc;
use stepvisualizer::common::types::{LengthUnit, Metadata, StepHeader};
use stepvisualizer::common::{FileId, StepModel};
use stepvisualizer::storage::LruCache;
use wasm_bindgen_test::*;

fn create_mock_model(id: &str) -> StepModel {
    StepModel {
        id: FileId::from(id),
        metadata: Metadata {
            header: StepHeader {
                file_description: "test".into(),
                implementation_level: "2;1".into(),
                file_name: format!("{id}.step").into(),
                time_stamp: "2026-09-01T00:00:00".into(),
                author: smallvec::SmallVec::from_vec(vec!["Author".into()]),
                organization: smallvec::SmallVec::from_vec(vec!["Org".into()]),
                preprocessor_version: "1.0".into(),
                originating_system: "TestSys".into(),
                authorization: "None".into(),
                file_schema: "AP214".into(),
            },
            entity_count: 10,
            bounding_box: None,
            units: Some(LengthUnit::Millimetre),
            vertex_count: 100,
            triangle_count: 50,
            volume: Some(100.0),
            surface_area: Some(250.0),
        },
        render_parts: vec![],
        part_visibility: vec![],
        visibility_generation: 0,
        cached_bounds: None,
        audit: stepvisualizer::common::AuditMetadata::default(),
    }
}

/// Verifies inserting a model into the cache allows retrieval with matching metadata and ID.
#[wasm_bindgen_test]
fn cache_insert_and_hit() {
    let mut cache = LruCache::new(5);
    let id = FileId::from("model_1");
    let model = create_mock_model("model_1");

    cache.insert(id.clone(), model);

    let cached = cache.get("model_1");
    assert!(cached.is_some());
    assert_eq!(cached.unwrap().id, id);
    assert!(cache.get("nonexistent").is_none());
}

/// Verifies that multiple cache hits return reference-counted Rc pointers to the
/// same underlying heap allocation without performing deep clones.
#[wasm_bindgen_test]
fn cache_rc_pointer_equality() {
    let mut cache = LruCache::new(5);
    let id = FileId::from("model_1");
    cache.insert(id, create_mock_model("model_1"));

    let rc1 = cache.get("model_1").expect("cache hit 1");
    let rc2 = cache.get("model_1").expect("cache hit 2");

    assert!(Rc::ptr_eq(&rc1, &rc2));
}

/// Verifies capacity-based LRU eviction and access-based promotion.
#[wasm_bindgen_test]
fn cache_eviction_and_lru_promotion() {
    let mut cache = LruCache::new(2);
    cache.insert(FileId::from("model_A"), create_mock_model("model_A"));
    cache.insert(FileId::from("model_B"), create_mock_model("model_B"));

    // Touch A to promote it to MRU
    assert!(cache.get("model_A").is_some());

    // Insert C -> B was LRU and should be evicted, while A and C remain
    cache.insert(FileId::from("model_C"), create_mock_model("model_C"));

    assert!(cache.get("model_A").is_some());
    assert!(cache.get("model_B").is_none());
    assert!(cache.get("model_C").is_some());
    assert_eq!(cache.len(), 2);
}

/// Verifies re-inserting an existing key updates payload without changing capacity or order.
#[wasm_bindgen_test]
fn cache_reinsert_existing_key() {
    let mut cache = LruCache::new(2);
    cache.insert(FileId::from("model_A"), create_mock_model("model_A"));
    cache.insert(FileId::from("model_B"), create_mock_model("model_B"));

    let mut updated_a = create_mock_model("model_A");
    updated_a.metadata.entity_count = 999;
    cache.insert(FileId::from("model_A"), updated_a);

    assert_eq!(cache.len(), 2);
    let a = cache.get("model_A").unwrap();
    assert_eq!(a.metadata.entity_count, 999);
    assert!(cache.get("model_B").is_some());
}

/// Verifies get_or_load invokes loader once on miss, caches result, and serves future calls from memory.
#[wasm_bindgen_test]
fn get_or_load_lifecycle() {
    let mut cache = LruCache::new(5);
    let load_count = Cell::new(0);

    let res1 = cache.get_or_load("model_A", |id| {
        load_count.set(load_count.get() + 1);
        Some(create_mock_model(id))
    });
    let res2 = cache.get_or_load("model_A", |id| {
        load_count.set(load_count.get() + 1);
        Some(create_mock_model(id))
    });

    assert!(res1.is_some());
    assert_eq!(load_count.get(), 1);
    assert!(Rc::ptr_eq(&res1.unwrap(), &res2.unwrap()));

    let miss = cache.get_or_load("model_missing", |_| None);
    assert!(miss.is_none());
}

/// Verifies that StepModel serializes and deserializes accurately via rkyv binary format.
#[wasm_bindgen_test]
fn step_model_rkyv_binary_roundtrip() {
    let model = create_mock_model("model_rkyv");
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&model).expect("rkyv serialization succeeds");
    assert!(!bytes.is_empty());

    // Simulate JS Uint8Array boundary as done in IndexedDB storage
    let uint8 = js_sys::Uint8Array::from(bytes.as_slice());
    let len = uint8.length() as usize;
    let mut aligned = rkyv::util::AlignedVec::<16>::with_capacity(len);
    aligned.resize(len, 0);
    uint8.copy_to(&mut aligned[..]);

    let deserialized: StepModel = rkyv::from_bytes::<StepModel, rkyv::rancor::Error>(&aligned)
        .expect("rkyv deserialization succeeds");

    assert_eq!(deserialized.id, model.id);
    assert_eq!(deserialized.metadata.header.file_name, "model_rkyv.step");
    assert_eq!(deserialized.metadata.entity_count, 10);
    assert_eq!(deserialized.metadata.vertex_count, 100);
    assert_eq!(deserialized.metadata.triangle_count, 50);
    assert_eq!(deserialized.part_visibility, model.part_visibility);
    assert_eq!(deserialized.audit.created_by, model.audit.created_by);
}
