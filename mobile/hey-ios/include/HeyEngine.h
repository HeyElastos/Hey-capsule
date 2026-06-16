// HeyEngine C-ABI — the contract between the Swift app and the Rust engine
// (capsules/hey-mobile-runtime). Mirrors the JNI surface in HeyApi.kt, but as a
// plain C ABI for Swift interop. The Rust side is src/ios.rs.
//
// Memory rules:
//   • every `char*` return is a heap JSON string OWNED BY THE CALLER — pass it to
//     hey_string_free() exactly once. NULL means an internal error.
//   • every `uint8_t*` return writes its length to `*out_len` and is OWNED BY THE
//     CALLER — pass (ptr, len) to hey_bytes_free() exactly once. NULL/0 = empty.
#ifndef HEY_ENGINE_H
#define HEY_ENGINE_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// ── lifecycle ────────────────────────────────────────────────────────────────
int   hey_set_storage_key(const char* dek_b64);  // install 32-byte at-rest DEK (Base64). 0 ok / -1 bad. CALL FIRST.
void  hey_start(const char* dir);     // bring up runtime + carrier + receivers; `dir` = vault/storage root (App Group)
void  hey_restore(const char* dir, const char* phrase); // boot deriving identity FROM phrase (restore / vault unseal)
void  hey_net_changed(void);          // re-probe carrier after a network change (NWPathMonitor)
void  hey_string_free(char* s);
void  hey_bytes_free(uint8_t* ptr, size_t len);

// ── identity / profile ───────────────────────────────────────────────────────
char* hey_whoami(void);                                   // {did, nickname}
char* hey_profile(const char* did);                       // "" = self → {did,nickname,bio,avatar}
char* hey_save_profile(const char* nickname, const char* bio, const char* avatar); // {} or {error}
char* hey_recovery_phrase(void);                          // runtime-held BIP39 phrase ("" if none)
char* hey_validate_mnemonic(const char* phrase);          // "ok" if valid, "" otherwise
char* hey_my_friend_link(void);                           // my follow-by-link invite
char* hey_gen_invite(const char* label);                  // one-time chat invite token
char* hey_accept_invite(const char* token);               // {ok,did} or {error}
char* hey_carrier_health(void);                           // {online,peers,relay,mode,…}

// ── feed ─────────────────────────────────────────────────────────────────────
char* hey_feed(int limit);                                // [Post]
char* hey_user_posts(const char* did);                    // [Post]
char* hey_get_post(const char* id);                       // Post or {error}
char* hey_upload_media(const uint8_t* data, size_t len, const char* mime, const char* name); // Media tile
char* hey_create_post(const char* text, const char* media_json); // media_json = JSON array ("" = text-only) → {} or {error}
char* hey_delete_post(const char* id);                    // {} or {error}
char* hey_edit_post(const char* id, const char* caption); // {} or {error}
char* hey_get_reactions(const char* post_id);             // {counts,mine,total}
char* hey_react(const char* post_id, const char* emoji);  // toggle/set → {counts,mine,total}
char* hey_get_comments(const char* post_id);              // [Comment]
char* hey_add_comment(const char* post_id, const char* text, const char* parent); // Comment
int64_t hey_feed_rev(void);                               // change counter (poll → reload on bump)

// ── chat ─────────────────────────────────────────────────────────────────────
char* hey_contacts(void);                                 // [Contact]
char* hey_groups(void);                                   // [Group]
char* hey_conversation(const char* did);                  // [Message] (1:1)
char* hey_group_conversation(const char* gid);            // [Message] (group)
char* hey_send_dm(const char* did, const char* text);     // {} or {error}
char* hey_send_group(const char* gid, const char* text);  // {} or {error}
char* hey_send_attachment(const char* did, const char* text, const uint8_t* data, size_t len, const char* mime, const char* filename);       // {} or {error}
char* hey_send_group_attachment(const char* gid, const char* text, const uint8_t* data, size_t len, const char* mime, const char* filename); // {} or {error}
uint8_t* hey_fetch_attachment(const char* att_json, size_t* out_len); // decrypted plaintext bytes
char* hey_react_message(const char* chat_id, const char* message_id, const char* emoji, int is_group);
char* hey_delete_message(const char* chat_id, const char* msg_id, int is_group); // {ok}
char* hey_edit_message(const char* chat_id, const char* msg_id, const char* text, int is_group); // {ok}
char* hey_message_reactions(const char* chat_id, int is_group); // [{message_id,emoji,sender_did}]
char* hey_create_group(const char* name, const char* members_json); // {id} or {error}
char* hey_start_chat(const char* did);                    // {} or {error}
char* hey_delete_conversation(const char* did);           // {} or {error}
char* hey_delete_group(const char* gid);                  // {} or {error}
void  hey_mark_read(const char* did);
int   hey_total_unread(void);
char* hey_peer_ticket(const char* did);                   // carrier ticket (base32), "" if unknown

// ── social graph ─────────────────────────────────────────────────────────────
char* hey_follow(const char* input);                      // did or invite link → {} or {error}
char* hey_unfollow(const char* did);                      // {} or {error}
char* hey_following(void);                                 // [Follow]
char* hey_followers(void);                                 // [Follow]
char* hey_follow_back(const char* did);                   // {} or {error}
char* hey_is_following(const char* did);                  // {following:bool}
char* hey_drain_notifs(void);                             // [HeyNotification]

// ── wallet (one BIP39 seed → all chains; runtime-held identity) ───────────────
// `mnemonic` "" tells Rust to resolve the runtime-held seed in-process (the iOS
// path, since the runtime is always up). MONEY sends need a one-shot spend grant
// (90s TTL, bound to kind+to+amount) minted by hey_authorize_spend.
char* hey_wallet_address(const char* mnemonic);                    // ESC 0x address ("" on error)
char* hey_wallet_chains(void);                                     // [{key,name,chainId,symbol}]
char* hey_wallet_balance(const char* mnemonic, const char* chain); // {address,balance,wei,symbol}
char* hey_wallet_balances(const char* mnemonic, const char* chain);// {address,tokens:[…]}
char* hey_wallet_check_address(const char* addr);                  // {ok,address} or {error}
char* hey_wallet_tx_status(const char* chain, const char* hash);   // {status}
char* hey_authorize_spend(const char* kind, const char* to, const char* amount); // {token} or {error}; kind = "ela"|"evm:<chain>"|"erc20:<chain>:<contract>"
char* hey_wallet_send(const char* mnemonic, const char* chain, const char* to, const char* value_hex, const char* auth); // value_hex = wei, no 0x → {txHash}|{error}
char* hey_wallet_token_send(const char* mnemonic, const char* chain, const char* contract, const char* to, const char* amount_hex, const char* auth); // {txHash}|{error}
char* hey_audit_log(int limit);                                    // newline-joined audit lines

// ── Elastos DID (EID) + ELA mainchain (same mnemonic, Essentials-recoverable) ─
char* hey_elastos_did(const char* mnemonic);                       // did:elastos:… ("" on error)
char* hey_ela_address(const char* mnemonic);                       // E… address ("" on error)
char* hey_ela_balance(const char* mnemonic);                       // {address,sela,ela}|{error}
char* hey_ela_send(const char* mnemonic, const char* to, const char* amount, const char* auth); // amount = decimal ELA → {txHash}|{error}

// ── tipping ──────────────────────────────────────────────────────────────────
char* hey_set_tip_addresses(const char* addresses_json);  // publish {chainKey:address} → {} or {error}
char* hey_resolve_tip(const char* did);                   // {chainKey:address} (or {})
char* hey_refresh_contact(const char* did);               // resolve + DM-exchange addresses
char* hey_notify_tip(const char* to, const char* sym, const char* amount, const char* txid); // {ok}

// ── content — resolve a media CID to raw bytes via the IN-PROCESS content provider
uint8_t* hey_content_bytes(const char* cid, size_t* out_len);     // caller frees with hey_bytes_free

// ── 1:1 voice calls + signaling ──────────────────────────────────────────────
char* hey_call_send(const char* did, const char* payload_json);   // {ok}
char* hey_call_poll(void);                                        // [{from,payload}]
void  hey_voice_start(const char* peer_ticket, int is_caller);
int   hey_voice_peers(void);
void  hey_voice_send(const uint8_t* pcm, size_t len);             // 16-bit LE PCM frame
uint8_t* hey_voice_recv(int max_bytes, size_t* out_len);         // decoded PCM; caller frees with hey_bytes_free
void  hey_voice_set_muted(int muted);
void  hey_voice_stop(void);

// ── HeyVerse lane (sealed + ratcheted; in-memory inbox) ──────────────────────
char* hey_verse_send(const char* did, const char* payload_json);  // {"ok":bool}
char* hey_verse_poll(void);                                       // [{from,payload}]

// ── push (iOS): register the device's APNs token + blinded handles with the gateway
void  hey_register_push_token(const char* apns_token_hex);

#ifdef __cplusplus
}
#endif

#endif // HEY_ENGINE_H
