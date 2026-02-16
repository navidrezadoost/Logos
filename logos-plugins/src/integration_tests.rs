//! End-to-end integration tests for the marketplace pipeline.
//!
//! Tests the complete flow:
//!   TOML manifest → Package → Sign → Verify → Registry Install →
//!   Permission Prompt → WASM Load → Execute
//!
//! These tests wire together multiple modules to confirm the
//! full plugin lifecycle works correctly.

#[cfg(test)]
mod tests {
    use crate::engine::WasmRuntime;
    use crate::examples::{
        auto_align_manifest, grid_generator_manifest, layer_info_manifest,
        AUTO_ALIGN_TOML, GRID_GENERATOR_WAT, LAYER_INFO_WAT,
    };
    use crate::manifest::PluginManifest;
    use crate::marketplace_http::{
        ApiResponse, MarketplaceHttpClient, TransactionState,
    };
    use crate::packaging::PluginPackage;
    use crate::permission_prompt::{
        PermissionPromptSession, RiskLevel, SavedPermissionPreferences,
    };
    use crate::permissions::PermissionKind;
    use crate::registry::{PluginRegistry, RegistrySource};
    use crate::runtime::ResourceLimits;
    use crate::signing::{
        PluginKeyPair, SignatureVerifier, SigningContext, VerificationPolicy,
    };
    use logos_core::Document;
    use std::sync::{Arc, RwLock};

    /// Dummy code bytes for packaging tests (packaging doesn't validate WASM).
    fn dummy_code() -> Vec<u8> {
        b"(module)".to_vec()
    }

    // ══════════════════════════════════════════════════════════
    // E2E: TOML → Package → Sign → Verify → Install
    // ══════════════════════════════════════════════════════════

    #[test]
    fn test_e2e_toml_to_signed_package() {
        // 1. Parse TOML manifest
        let manifest = PluginManifest::from_toml_str(AUTO_ALIGN_TOML).unwrap();
        assert_eq!(manifest.name, "Auto-Align");

        // 2. Create package from manifest + code
        let mut package = PluginPackage::create(&manifest, &dummy_code()).unwrap();

        // 3. Sign the package
        let keypair = PluginKeyPair::generate();
        package.sign(&keypair);
        assert!(package.flags.signed);

        // 4. Verify signature
        package.verify_signature().unwrap();

        // 5. Verify integrity
        package.verify_integrity().unwrap();
    }

    #[test]
    fn test_e2e_package_serialize_deserialize() {
        let manifest = auto_align_manifest();
        let mut package = PluginPackage::create(&manifest, &dummy_code()).unwrap();

        let keypair = PluginKeyPair::generate();
        package.sign(&keypair);

        // Serialize to bytes
        let bytes = package.to_bytes().unwrap();
        assert!(!bytes.is_empty());

        // Deserialize back
        let restored = PluginPackage::from_bytes(&bytes).unwrap();
        assert_eq!(restored.manifest.name, "Auto-Align");
        assert!(restored.flags.signed);
        restored.verify_integrity().unwrap();
    }

    #[test]
    fn test_e2e_registry_install_signed_plugin() {
        let manifest = auto_align_manifest();
        let mut package = PluginPackage::create(&manifest, &dummy_code()).unwrap();

        let keypair = PluginKeyPair::generate();
        package.sign(&keypair);

        // Install into registry
        let mut registry = PluginRegistry::new();
        registry.install(&package, RegistrySource::Marketplace).unwrap();

        // Verify it's installed
        let found = registry.find_by_name("Auto-Align");
        assert!(found.is_some());
    }

    #[test]
    fn test_e2e_registry_rejects_unsigned_when_required() {
        let manifest = auto_align_manifest();
        let package = PluginPackage::create(&manifest, &dummy_code()).unwrap();

        let mut registry = PluginRegistry::new();
        registry.set_require_signatures(true);

        // Should fail — package is unsigned
        let result = registry.install(&package, RegistrySource::Marketplace);
        assert!(result.is_err());
    }

    // ══════════════════════════════════════════════════════════
    // E2E: Permission Prompt → Install Approval
    // ══════════════════════════════════════════════════════════

    #[test]
    fn test_e2e_permission_review_and_install() {
        let manifest = auto_align_manifest();

        // Build permission prompt
        let mut session = PermissionPromptSession::from_manifest(&manifest);
        assert_eq!(session.items.len(), 2);
        assert_eq!(session.overall_risk, RiskLevel::Medium);

        // User grants all permissions
        session.grant_all();
        let approval = session.finalize().unwrap();
        assert!(approval.approved);
        assert_eq!(approval.granted_count(), 2);

        // Build and install package
        let keypair = PluginKeyPair::generate();
        let mut package = PluginPackage::create(&manifest, &dummy_code()).unwrap();
        package.sign(&keypair);

        let mut registry = PluginRegistry::new();
        registry.install(&package, RegistrySource::Marketplace).unwrap();
        assert!(registry.find_by_name("Auto-Align").is_some());
    }

    #[test]
    fn test_e2e_permission_denial_blocks_install() {
        let manifest = auto_align_manifest();

        let mut session = PermissionPromptSession::from_manifest(&manifest);
        session.deny_all();
        let approval = session.finalize().unwrap();
        assert!(!approval.approved);
    }

    #[test]
    fn test_e2e_saved_preferences_carried_forward() {
        let manifest = auto_align_manifest();

        // First install: user grants
        let mut session1 = PermissionPromptSession::from_manifest(&manifest);
        session1.grant_all();
        let approval1 = session1.finalize().unwrap();

        let mut prefs = SavedPermissionPreferences::new();
        prefs.save_from_approval(&approval1);

        // Second install (update): preferences auto-applied
        let mut session2 = PermissionPromptSession::from_manifest(&manifest);
        let applied = prefs.apply_to_session(&mut session2);
        assert_eq!(applied, 2);
        assert!(session2.all_decided());
        assert!(session2.all_required_granted());
    }

    // ══════════════════════════════════════════════════════════
    // E2E: Verification Pipeline
    // ══════════════════════════════════════════════════════════

    #[test]
    fn test_e2e_verification_pipeline_full() {
        let manifest = auto_align_manifest();
        let code = dummy_code();
        let mut package = PluginPackage::create(&manifest, &code).unwrap();

        let keypair = PluginKeyPair::generate();
        package.sign(&keypair);

        // Verify with the full pipeline
        let sig = package.signature.as_ref().unwrap();
        let mut verifier = SignatureVerifier::new(VerificationPolicy::Full);
        verifier.trust_key(keypair.public_key().to_hex());

        let result = verifier.verify(&package.manifest_json, &code, sig);
        assert!(result.passed, "verification failed: {:?}", result);
        assert!(result.signer_trusted);
    }

    #[test]
    fn test_e2e_verification_untrusted_publisher() {
        let manifest = grid_generator_manifest();
        let code = dummy_code();
        let mut package = PluginPackage::create(&manifest, &code).unwrap();

        let keypair = PluginKeyPair::generate();
        package.sign(&keypair);

        // Verify with a DIFFERENT trusted key so the signer is untrusted
        // (empty trusted_keys = open trust model, which passes)
        let sig = package.signature.as_ref().unwrap();
        let mut verifier = SignatureVerifier::new(VerificationPolicy::Full);
        verifier.trust_key("aaaa".repeat(16)); // trust a different key

        let result = verifier.verify(&package.manifest_json, &code, sig);
        assert!(!result.passed);
        assert!(!result.signer_trusted);
    }

    // ══════════════════════════════════════════════════════════
    // E2E: Signing Context
    // ══════════════════════════════════════════════════════════

    #[test]
    fn test_e2e_signing_context_roundtrip() {
        let ctx = SigningContext::new();
        let manifest = auto_align_manifest();
        let code = dummy_code();

        let manifest_json = serde_json::to_vec(&manifest).unwrap();
        let sig = ctx.sign_plugin(&manifest_json, &code);

        let result = SigningContext::verify_plugin(&manifest_json, &code, &sig);
        assert!(result.is_ok());
    }

    // ══════════════════════════════════════════════════════════
    // E2E: HTTP Client Transaction Flow
    // ══════════════════════════════════════════════════════════

    #[test]
    fn test_e2e_http_transaction_install_flow() {
        let manifest = grid_generator_manifest();
        let mut client = MarketplaceHttpClient::new();

        // Begin transaction
        let tx = client.begin_install(manifest.id, &manifest.name, manifest.version.clone());
        let tx_id = tx.id;
        assert!(tx.is_active());

        // Start download
        let dl_idx = client.begin_download(manifest.id, 4096);
        client.update_download(dl_idx, 2048);
        assert_eq!(client.download_progress(dl_idx).unwrap().percent(), 50);

        // Advance transaction through stages
        let tx = client.transaction_mut(&tx_id).unwrap();
        tx.advance(TransactionState::Downloading, "downloading 4KB");
        tx.advance(TransactionState::Verifying, "verifying signature");
        tx.advance(TransactionState::AwaitingPermissions, "waiting for user");
        tx.advance(TransactionState::Installing, "installing to registry");
        tx.commit();

        assert!(client.transaction(&tx_id).unwrap().is_committed());
        assert!(client.active_transactions().is_empty());
    }

    #[test]
    fn test_e2e_http_transaction_rollback() {
        let manifest = layer_info_manifest();
        let mut client = MarketplaceHttpClient::new();

        let tx = client.begin_install(manifest.id, &manifest.name, manifest.version.clone());
        let tx_id = tx.id;

        let tx = client.transaction_mut(&tx_id).unwrap();
        tx.advance(TransactionState::Downloading, "starting");
        tx.rollback("signature verification failed");

        let tx = client.transaction(&tx_id).unwrap();
        assert_eq!(tx.state, TransactionState::RolledBack);
        assert!(tx.error.is_some());
    }

    // ══════════════════════════════════════════════════════════
    // E2E: WASM Load + Execute
    // ══════════════════════════════════════════════════════════

    #[test]
    fn test_e2e_grid_generator_full_pipeline() {
        // 1. Build manifest
        let manifest = grid_generator_manifest();

        // 2. Permission check
        let mut session = PermissionPromptSession::from_manifest(&manifest);
        session.grant_all();
        let approval = session.finalize().unwrap();
        assert!(approval.approved);

        // 3. Package + sign
        let keypair = PluginKeyPair::generate();
        let mut package = PluginPackage::create(&manifest, &dummy_code()).unwrap();
        package.sign(&keypair);

        // 4. Install
        let mut registry = PluginRegistry::new();
        registry.install(&package, RegistrySource::Marketplace).unwrap();

        // 5. Load and execute WASM (using WAT source directly)
        let mut rt = WasmRuntime::new(
            "grid-generator",
            ResourceLimits::default(),
            approval.granted_permissions.clone(),
        )
        .unwrap();
        rt.load_wat(GRID_GENERATOR_WAT).unwrap();
        rt.register_document(Arc::new(RwLock::new(Document::new())));
        let result = rt.execute("generate").unwrap();

        // Grid generator returns 9 (3x3 grid)
        assert!(matches!(result, crate::runtime::PluginValue::Int(9)));
    }

    #[test]
    fn test_e2e_layer_info_full_pipeline() {
        let manifest = layer_info_manifest();

        let mut session = PermissionPromptSession::from_manifest(&manifest);
        session.grant_all();
        let approval = session.finalize().unwrap();
        assert!(approval.approved);

        let keypair = PluginKeyPair::generate();
        let mut package = PluginPackage::create(&manifest, &dummy_code()).unwrap();
        package.sign(&keypair);

        let mut registry = PluginRegistry::new();
        registry.install(&package, RegistrySource::Marketplace).unwrap();

        let mut rt = WasmRuntime::new(
            "layer-info",
            ResourceLimits::default(),
            approval.granted_permissions.clone(),
        )
        .unwrap();
        rt.load_wat(LAYER_INFO_WAT).unwrap();
        rt.register_document(Arc::new(RwLock::new(Document::new())));
        let result = rt.execute("info");
        assert!(result.is_ok());
    }

    // ══════════════════════════════════════════════════════════
    // E2E: Multi-plugin Marketplace Scenario
    // ══════════════════════════════════════════════════════════

    #[test]
    fn test_e2e_multi_plugin_install() {
        let mut registry = PluginRegistry::new();
        let keypair = PluginKeyPair::generate();

        let manifests = vec![
            auto_align_manifest(),
            layer_info_manifest(),
            grid_generator_manifest(),
        ];

        for manifest in &manifests {
            let mut package = PluginPackage::create(manifest, &dummy_code()).unwrap();
            package.sign(&keypair);
            registry.install(&package, RegistrySource::Marketplace).unwrap();
        }

        assert!(registry.find_by_name("Auto-Align").is_some());
        assert!(registry.find_by_name("Layer Info").is_some());
        assert!(registry.find_by_name("Grid Generator").is_some());
    }

    #[test]
    fn test_e2e_api_response_json_roundtrip() {
        let response = ApiResponse::success("plugin data".to_string());
        let json = serde_json::to_string(&response).unwrap();
        let parsed: ApiResponse<String> = serde_json::from_str(&json).unwrap();
        assert!(parsed.success);
        assert_eq!(parsed.data, Some("plugin data".to_string()));
    }

    #[test]
    fn test_e2e_critical_risk_network_write_combo() {
        let mut manifest = auto_align_manifest();
        manifest.permissions.grant(PermissionKind::Network);

        let session = PermissionPromptSession::from_manifest(&manifest);
        assert_eq!(session.overall_risk, RiskLevel::Critical);
    }
}
