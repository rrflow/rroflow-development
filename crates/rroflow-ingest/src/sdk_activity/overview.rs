//! Metadata-only overview; never reads captured prompt, response or tool content.
use super::*;

pub(crate) async fn get(
    State(st): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let ds = st
        .allowed(&headers, crate::Permission::ProjectRead)
        .await?
        .devspace;
    let Store::Postgres(pg) = st.store.as_ref() else {
        return Ok(Json(
            json!({"available":false,"devspace":ds,"reason":"SDK activity indexing is unavailable on this storage backend."}),
        ));
    };
    let mut tx = pg.pool.begin().await.map_err(|e| internal(e.into()))?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await
        .map_err(|e| internal(e.into()))?;
    let totals = sqlx::query("SELECT count(*) AS activities,count(*) FILTER(WHERE cardinality(signals)>0) AS with_findings,max(published_at) AS indexed_at FROM sdk_activities WHERE devspace_id=$1")
        .bind(&ds).fetch_one(&mut *tx).await.map_err(|e|internal(e.into()))?;
    let agents = sqlx::query("SELECT agent,count(*) AS activities FROM sdk_activities WHERE devspace_id=$1 GROUP BY agent ORDER BY count(*) DESC,agent LIMIT 12")
        .bind(&ds).fetch_all(&mut *tx).await.map_err(|e|internal(e.into()))?;
    let signals = sqlx::query("SELECT signal,count(DISTINCT a.activity_id) AS activities FROM sdk_activities a CROSS JOIN LATERAL unnest(a.signals) AS signal WHERE a.devspace_id=$1 GROUP BY signal ORDER BY count(DISTINCT a.activity_id) DESC,signal LIMIT 12")
        .bind(&ds).fetch_all(&mut *tx).await.map_err(|e|internal(e.into()))?;
    let coverage = sqlx::query("SELECT count(*) FILTER(WHERE status<>'ready') AS pending,count(*) FILTER(WHERE status='error') AS errors FROM sdk_activity_work WHERE devspace_id=$1")
        .bind(&ds).fetch_one(&mut *tx).await.map_err(|e|internal(e.into()))?;
    let agents:Vec<Value>=agents.into_iter().map(|r|json!({"agent":r.get::<String,_>("agent"),"activities":r.get::<i64,_>("activities")})).collect();
    let signals:Vec<Value>=signals.into_iter().map(|r|json!({"signal":r.get::<String,_>("signal"),"activities":r.get::<i64,_>("activities")})).collect();
    let count: i64 = totals.get("activities");
    let represented: i64 = agents
        .iter()
        .map(|a| a["activities"].as_i64().unwrap())
        .sum();
    let indexed = totals
        .get::<Option<chrono::DateTime<chrono::Utc>>, _>("indexed_at")
        .map(|v| v.to_rfc3339());
    tx.commit().await.map_err(|e| internal(e.into()))?;
    Ok(Json(
        json!({"available":true,"devspace":ds,"activities":count,"activities_with_findings":totals.get::<i64,_>("with_findings"),
        "by_agent":agents,"other_agent_activities":count-represented,"top_signals":signals,"breakdown_limit":12,
        "coverage":{"pending_captures":coverage.get::<i64,_>("pending"),"error_captures":coverage.get::<i64,_>("errors")},
        "indexed_at":indexed,"as_of":chrono::Utc::now().to_rfc3339()}),
    ))
}
