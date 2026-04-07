use refinery_protocol::{
    PRIVATE_KEY_LENGTH, SMPC_PROTOCOL_NAME, SMPC_PROTOCOL_VERSION, SharePayload,
    decrypt_share_payload, encrypt_share_payload, public_key_from_private_key,
    split_additive_shares, sum_slot_vectors,
};

#[test]
fn additive_shares_reconstruct_original_slot_vector() {
    let slots = vec![5u64, u64::MAX - 1, 44u64];
    let shares = split_additive_shares(&slots, 3).expect("shares should split");
    let reconstructed = sum_slot_vectors(&shares).expect("shares should sum");
    assert_eq!(reconstructed, slots);
}

#[test]
fn encrypted_payload_round_trip_works() {
    let sender_private = [7u8; PRIVATE_KEY_LENGTH];
    let recipient_private = [9u8; PRIVATE_KEY_LENGTH];
    let recipient_public = public_key_from_private_key(&recipient_private);
    let sender_public = public_key_from_private_key(&sender_private);

    let payload = SharePayload {
        job_id: "job-1".to_string(),
        job_context_hash: "hash".to_string(),
        protocol_name: SMPC_PROTOCOL_NAME.to_string(),
        protocol_version: SMPC_PROTOCOL_VERSION.to_string(),
        sender_node_id: "node-a".to_string(),
        recipient_node_id: "node-b".to_string(),
        schema_id: "schema".to_string(),
        slot_labels: vec!["a".to_string()],
        slot_bytes: vec![1, 2, 3],
    };

    let (nonce, ciphertext) =
        encrypt_share_payload(&sender_private, &recipient_public, &payload).expect("encrypt");
    let decrypted =
        decrypt_share_payload(&recipient_private, &sender_public, &nonce, &ciphertext)
            .expect("decrypt");

    assert_eq!(decrypted, payload);
}
