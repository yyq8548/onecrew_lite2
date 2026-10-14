use onecrew_creator_runtime::{
    BulkMoveCreatorsToGroupInput, CREATOR_BUNDLE_XLSX_MAX_BYTES,
    ConfirmCreatorContactRouteDeleteInput, ConfirmCreatorContactRouteDeleteResult,
    ConfirmCreatorImportInput, ConfirmCreatorImportResult, CreateCreatorGroupInput, Creator,
    CreatorDeleteResult, CreatorGroup, CreatorGroupListResult, CreatorId, CreatorImportConflict,
    CreatorImportImpact, CreatorImportOperationId, CreatorImportReceipt, CreatorListPage,
    CreatorRecord, CreatorRelationship, DeleteCreatorGroupInput, DeleteCreatorGroupResult,
    DeleteCreatorInput, ListCreatorGroupsInput, ListCreatorsInput,
    ManualCreatorContactRouteAddInput, ManualCreatorContactRouteAddResult,
    ManualCreatorContactRouteUpdateInput, ManualCreatorContactRouteUpdateResult,
    ManualCreatorCreateInput, ManualCreatorCreateResult, MoveCreatorsToGroupResult,
    PrepareCreatorContactRouteDeleteInput, PrepareCreatorContactRouteDeleteResult,
    PrepareCreatorImportResult, SetCreatorGroupInput, SetCreatorPinnedInput,
    UpdateCreatorGroupInput, UpdateCreatorInput, UpdateCreatorStageInput,
};
use serde::Serialize;

use onecrew_local_runtime::{CreatorExportInput, CreatorExportResult};

use crate::ServiceState;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub(super) enum CreatorImportFilePrepareResult {
    AlreadyImported {
        file_name: String,
        receipt: CreatorImportReceipt,
    },
    Blocked {
        file_name: String,
        content_sha256: String,
        conflict_count: u64,
        conflicts: Vec<CreatorImportConflict>,
    },
    ConfirmationRequired {
        file_name: String,
        preview_rows: Vec<onecrew_creator_runtime::CreatorTransferCreator>,
        operation_id: CreatorImportOperationId,
        content_sha256: String,
        impact: CreatorImportImpact,
        expires_at: u64,
    },
}

pub(super) async fn onecrew_creator_export(
    state: &ServiceState,
    input: CreatorExportInput,
) -> Result<CreatorExportResult, String> {
    let platform = state.platform().await?;
    platform
        .export_creators_to_system_desktop(input)
        .await
        .map_err(|error| error.to_string())
}

pub(super) async fn onecrew_creator_import_file_prepare(
    state: &ServiceState,
    file_name: String,
    bytes: Vec<u8>,
) -> Result<CreatorImportFilePrepareResult, String> {
    if bytes.len() > CREATOR_BUNDLE_XLSX_MAX_BYTES {
        return Err("Creator workbook exceeds the 32 MiB upload limit".into());
    }
    if file_name.len() > 255
        || file_name.contains(['/', '\\', '\0'])
        || !file_name.to_ascii_lowercase().ends_with(".xlsx")
    {
        return Err("Creator import requires an .xlsx file name".into());
    }
    let creators = state.platform().await?.creators()?;
    let (prepared, preview_rows) = creators
        .prepare_creator_import_xlsx_preview_async(bytes)
        .await
        .map_err(|error| error.to_string())?;
    let mut result = map_file_prepare_result(file_name, prepared);
    if let CreatorImportFilePrepareResult::ConfirmationRequired { preview_rows: rows, .. } = &mut result { *rows = preview_rows; }
    Ok(result)
}

pub(super) async fn onecrew_creator_import_confirm(
    state: &ServiceState,
    input: ConfirmCreatorImportInput,
) -> Result<ConfirmCreatorImportResult, String> {
    let creators = state.platform().await?.creators()?;
    creators
        .confirm_creator_import_async(input)
        .await
        .map_err(|error| error.to_string())
}

pub(super) async fn onecrew_creator_import_cancel(
    state: &ServiceState,
    input: ConfirmCreatorImportInput,
) -> Result<ConfirmCreatorImportResult, String> {
    let creators = state.platform().await?.creators()?;
    creators
        .cancel_creator_import_async(input)
        .await
        .map_err(|error| error.to_string())
}

fn map_file_prepare_result(
    file_name: String,
    result: PrepareCreatorImportResult,
) -> CreatorImportFilePrepareResult {
    match result {
        PrepareCreatorImportResult::AlreadyImported { receipt } => {
            CreatorImportFilePrepareResult::AlreadyImported { file_name, receipt }
        }
        PrepareCreatorImportResult::Blocked {
            content_sha256,
            conflict_count,
            conflicts,
        } => CreatorImportFilePrepareResult::Blocked {
            file_name,
            content_sha256,
            conflict_count,
            conflicts,
        },
        PrepareCreatorImportResult::ConfirmationRequired {
            operation_id,
            content_sha256,
            impact,
            expires_at,
        } => CreatorImportFilePrepareResult::ConfirmationRequired {
            file_name,
            preview_rows: Vec::new(),
            operation_id,
            content_sha256,
            impact,
            expires_at,
        },
    }
}

pub(super) async fn onecrew_creator_list(
    state: &ServiceState,
    input: ListCreatorsInput,
) -> Result<CreatorListPage, String> {
    let creators = state.platform().await?.creators()?;
    creators
        .list_creators_async(input)
        .await
        .map_err(|error| error.to_string())
}

pub(super) async fn onecrew_creator_get(
    state: &ServiceState,
    input: onecrew_creator_runtime::CreatorIdInput,
) -> Result<CreatorRecord, String> {
    let creators = state.platform().await?.creators()?;
    creators
        .get_creator_async(input.creator_id)
        .await
        .map_err(|error| error.to_string())
}

pub(super) async fn onecrew_creator_group_list(
    state: &ServiceState,
    input: ListCreatorGroupsInput,
) -> Result<CreatorGroupListResult, String> {
    let creators = state.platform().await?.creators()?;
    creators
        .list_creator_groups_async(input)
        .await
        .map_err(|error| error.to_string())
}

pub(super) async fn onecrew_creator_group_create(
    state: &ServiceState,
    input: CreateCreatorGroupInput,
) -> Result<CreatorGroup, String> {
    let creators = state.platform().await?.creators()?;
    creators
        .create_creator_group_async(input)
        .await
        .map_err(|error| error.to_string())
}

pub(super) async fn onecrew_creator_group_update(
    state: &ServiceState,
    input: UpdateCreatorGroupInput,
) -> Result<CreatorGroup, String> {
    let creators = state.platform().await?.creators()?;
    creators
        .update_creator_group_async(input)
        .await
        .map_err(|error| error.to_string())
}

pub(super) async fn onecrew_creator_group_delete(
    state: &ServiceState,
    input: DeleteCreatorGroupInput,
) -> Result<DeleteCreatorGroupResult, String> {
    let creators = state.platform().await?.creators()?;
    creators
        .delete_creator_group_async(input)
        .await
        .map_err(|error| error.to_string())
}

pub(super) async fn onecrew_creator_group_set(
    state: &ServiceState,
    input: SetCreatorGroupInput,
) -> Result<MoveCreatorsToGroupResult, String> {
    let creators = state.platform().await?.creators()?;
    creators
        .set_creator_group_async(input)
        .await
        .map_err(|error| error.to_string())
}

pub(super) async fn onecrew_creator_group_bulk_move(
    state: &ServiceState,
    input: BulkMoveCreatorsToGroupInput,
) -> Result<MoveCreatorsToGroupResult, String> {
    let creators = state.platform().await?.creators()?;
    creators
        .bulk_move_creators_to_group_async(input)
        .await
        .map_err(|error| error.to_string())
}

pub(super) async fn onecrew_creator_manual_create(
    state: &ServiceState,
    input: ManualCreatorCreateInput,
) -> Result<ManualCreatorCreateResult, String> {
    let creators = state.platform().await?.creators()?;
    creators
        .create_manual_creator_async(input)
        .await
        .map_err(|error| error.to_string())
}

pub(super) async fn onecrew_creator_contact_route_add(
    state: &ServiceState,
    input: ManualCreatorContactRouteAddInput,
) -> Result<ManualCreatorContactRouteAddResult, String> {
    let creators = state.platform().await?.creators()?;
    creators
        .add_manual_creator_contact_route_async(input)
        .await
        .map_err(|error| error.to_string())
}

pub(super) async fn onecrew_creator_contact_route_update(
    state: &ServiceState,
    input: ManualCreatorContactRouteUpdateInput,
) -> Result<ManualCreatorContactRouteUpdateResult, String> {
    let creators = state.platform().await?.creators()?;
    creators
        .update_manual_creator_contact_route_async(input)
        .await
        .map_err(|error| error.to_string())
}

pub(super) async fn onecrew_creator_contact_route_delete_prepare(
    state: &ServiceState,
    input: PrepareCreatorContactRouteDeleteInput,
) -> Result<PrepareCreatorContactRouteDeleteResult, String> {
    let creators = state.platform().await?.creators()?;
    creators
        .prepare_creator_contact_route_delete_async(input)
        .await
        .map_err(|error| error.to_string())
}

pub(super) async fn onecrew_creator_contact_route_delete_confirm(
    state: &ServiceState,
    input: ConfirmCreatorContactRouteDeleteInput,
) -> Result<ConfirmCreatorContactRouteDeleteResult, String> {
    let creators = state.platform().await?.creators()?;
    creators
        .confirm_creator_contact_route_delete_async(input)
        .await
        .map_err(|error| error.to_string())
}

pub(super) async fn onecrew_creator_pin(
    state: &ServiceState,
    input: SetCreatorPinnedInput,
) -> Result<(), String> {
    let creators = state.platform().await?.creators()?;
    creators
        .set_creator_pinned_async(input)
        .await
        .map_err(|error| error.to_string())
}

pub(super) async fn onecrew_creator_update_stage(
    state: &ServiceState,
    input: UpdateCreatorStageInput,
) -> Result<CreatorRelationship, String> {
    let creators = state.platform().await?.creators()?;
    creators
        .update_creator_stage_async(input)
        .await
        .map_err(|error| error.to_string())
}

pub(super) async fn onecrew_creator_update(
    state: &ServiceState,
    input: UpdateCreatorInput,
) -> Result<CreatorRecord, String> {
    let creators = state.platform().await?.creators()?;
    creators
        .update_creator_async(input)
        .await
        .map_err(|error| error.to_string())
}

pub(super) async fn onecrew_creator_archive(
    state: &ServiceState,
    creator_id: String,
) -> Result<Creator, String> {
    let creators = state.platform().await?.creators()?;
    creators
        .archive_creator_async(CreatorId::new(creator_id))
        .await
        .map_err(|error| error.to_string())
}

pub(super) async fn onecrew_creator_delete(
    state: &ServiceState,
    input: DeleteCreatorInput,
) -> Result<CreatorDeleteResult, String> {
    let creators = state.platform().await?.creators()?;
    creators
        .delete_creator_async(input)
        .await
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn file_prepare_projection_never_adds_a_path() {
        let result = map_file_prepare_result(
            "portable.xlsx".into(),
            PrepareCreatorImportResult::Blocked {
                content_sha256: format!("sha256:{}", "01".repeat(32)),
                conflict_count: 0,
                conflicts: Vec::new(),
            },
        );
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("portable.xlsx"));
        assert!(!json.contains("/Users/"));
        assert!(!json.contains("C:\\\\"));
    }
}
