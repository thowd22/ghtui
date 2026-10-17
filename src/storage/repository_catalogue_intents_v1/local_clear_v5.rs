//! LOCAL data erasure, not current GitHub access or a source renewal. The
//! explicit queue intent retains its original genuine source/file boundary.
use super::*;
impl Cache {
    pub(crate) fn clear_original_catalogue_records_v5(
        original: &CatalogueIntentSource,
        database: &std::path::Path,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> BoundaryResult<()> {
        live(deadline, cancel)?;
        original
            .reservation
            .matches_context(original.source.context())?;
        let canonical =
            std::fs::canonicalize(database).map_err(|_| CatalogueIntentBoundaryError::Database)?;
        if file_issuer(&canonical)? != original.reservation.issuer {
            return Err(CatalogueIntentBoundaryError::OriginalSource);
        }
        live(deadline, cancel)?;
        let mut cache =
            Cache::open(database).map_err(|_| CatalogueIntentBoundaryError::Database)?;
        if issuer(&cache)? != original.reservation.issuer {
            return Err(CatalogueIntentBoundaryError::OriginalSource);
        }
        cache
            .connection
            .busy_timeout(
                deadline
                    .saturating_duration_since(Instant::now())
                    .min(std::time::Duration::from_secs(2)),
            )
            .map_err(|_| CatalogueIntentBoundaryError::Database)?;
        let tx = cache
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| CatalogueIntentBoundaryError::Database)?;
        // A queue delay/lock wait uses the ORIGINAL deadline. Source/Clear,
        // checkpoint and exact saved payload cannot be rebased under this lock.
        live(deadline, cancel)?;
        validate_epoch(&tx, &original.reservation)?;
        validate_payload(&tx, &original.reservation)?;
        if file_issuer(&original.reservation.issuer.path)? != original.reservation.issuer {
            return Err(CatalogueIntentBoundaryError::OriginalSource);
        }
        Cache::clear_records_transaction_v5(&tx, original.scope())
            .map_err(|_| CatalogueIntentBoundaryError::Database)?;
        live(deadline, cancel)?;
        if file_issuer(&original.reservation.issuer.path)? != original.reservation.issuer {
            return Err(CatalogueIntentBoundaryError::OriginalSource);
        }
        // Linearization: once this commit succeeds the user's explicit local
        // Clear occurred, even if a subsequent navigation declines its paint.
        tx.commit()
            .map_err(|_| CatalogueIntentBoundaryError::Database)
    }
}
