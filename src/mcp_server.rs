use rmcp::{
    ServerHandler, ServiceExt,
    handler::server::router::tool::ToolRouter,
    model::{ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router,
    transport::stdio,
};

use crate::todo;

#[derive(Debug, Clone)]
pub struct TorudoMcpServer {
    todotxt_dir: String,
    todo_file: String,
    tool_router: ToolRouter<Self>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ListTodosParams {
    #[schemars(description = "Filter by project name (without + prefix)")]
    pub project: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct RegisterPlanParams {
    #[schemars(description = "Description of the task")]
    pub description: String,
    #[schemars(
        description = "Plan content to write to the detail markdown file. Supports YAML frontmatter with 'tmux_pane' key (e.g., '---\\ntmux_pane: 0:1.2\\n---\\n# Plan content'). The tmux_pane value specifies which tmux pane to jump to when the todo is selected in the TUI."
    )]
    pub plan: String,
    #[schemars(
        description = "Project tag (without + prefix). Use the current repository/directory name."
    )]
    pub project: String,
    #[schemars(description = "Priority (A-Z). Omit for no priority.")]
    pub priority: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct UpdatePlanParams {
    #[schemars(description = "ID of the todo item to update (from register_plan response or list_todos)")]
    pub id: String,
    #[schemars(
        description = "New plan content to overwrite the existing detail markdown file. Supports YAML frontmatter with 'tmux_pane' key (e.g., '---\\ntmux_pane: 0:1.2\\n---\\n# Plan content'). The tmux_pane value specifies which tmux pane to jump to when the todo is selected in the TUI."
    )]
    pub plan: String,
}

#[tool_router]
impl TorudoMcpServer {
    pub fn new(todotxt_dir: String, todo_file: String) -> Self {
        Self {
            todotxt_dir,
            todo_file,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(description = "List all todos from todo.txt. Optionally filter by project.")]
    fn list_todos(
        &self,
        rmcp::handler::server::wrapper::Parameters(params): rmcp::handler::server::wrapper::Parameters<ListTodosParams>,
    ) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        let todos = todo::load_todos(&self.todo_file).map_err(|e| rmcp::ErrorData {
            code: rmcp::model::ErrorCode::INTERNAL_ERROR,
            message: format!("Failed to load todos: {e}").into(),
            data: None,
        })?;

        let filtered: Vec<&todo::Item> = params.project.as_ref().map_or_else(
            || todos.iter().collect(),
            |project| todos.iter().filter(|t| t.projects.contains(project)).collect(),
        );

        let items: Vec<serde_json::Value> = filtered
            .iter()
            .map(|t| {
                serde_json::json!({
                    "id": t.id,
                    "description": t.description,
                    "priority": t.priority.map(|c| c.to_string()),
                    "projects": t.projects,
                    "contexts": t.contexts,
                    "completed": t.completed,
                })
            })
            .collect();

        let json = serde_json::to_string_pretty(&items).unwrap_or_default();
        Ok(rmcp::model::CallToolResult::success(vec![
            rmcp::model::Content::text(json),
        ]))
    }

    #[tool(
        description = "Register a new plan as a todo item. Creates a todo.txt entry and a detail markdown file."
    )]
    fn register_plan(
        &self,
        rmcp::handler::server::wrapper::Parameters(params): rmcp::handler::server::wrapper::Parameters<RegisterPlanParams>,
    ) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        let priority = params.priority.and_then(|p| {
            let c = p.chars().next()?;
            if c.is_ascii_uppercase() { Some(c) } else { None }
        });

        let item = todo::append_todo(
            &self.todo_file,
            &self.todotxt_dir,
            &params.description,
            &params.project,
            priority,
            Some(&params.plan),
        )
        .map_err(|e| rmcp::ErrorData {
            code: rmcp::model::ErrorCode::INTERNAL_ERROR,
            message: format!("Failed to register plan: {e}").into(),
            data: None,
        })?;

        let result = serde_json::json!({
            "id": item.id,
            "description": item.description,
            "priority": item.priority.map(|c| c.to_string()),
            "projects": item.projects,
            "contexts": item.contexts,
            "completed": item.completed,
        });

        let json = serde_json::to_string_pretty(&result).unwrap_or_default();
        Ok(rmcp::model::CallToolResult::success(vec![
            rmcp::model::Content::text(json),
        ]))
    }

    #[tool(
        description = "Update an existing plan's detail markdown file. Use the ID from register_plan response or list_todos."
    )]
    fn update_plan(
        &self,
        rmcp::handler::server::wrapper::Parameters(params): rmcp::handler::server::wrapper::Parameters<UpdatePlanParams>,
    ) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        todo::update_plan(&self.todotxt_dir, &params.id, &params.plan).map_err(|e| {
            rmcp::ErrorData {
                code: rmcp::model::ErrorCode::INTERNAL_ERROR,
                message: format!("Failed to update plan: {e}").into(),
                data: None,
            }
        })?;

        let result = serde_json::json!({ "id": params.id });
        let json = serde_json::to_string_pretty(&result).unwrap_or_default();
        Ok(rmcp::model::CallToolResult::success(vec![
            rmcp::model::Content::text(json),
        ]))
    }
}

#[tool_handler]
impl ServerHandler for TorudoMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "torudo MCP server - manage todo.txt items and plans for GTD workflow. \
                 When registering or updating plans, if the TMUX environment variable is set, \
                 automatically detect the current pane by running \
                 `tmux display-message -p '#{session_name}:#{window_index}.#{pane_index}'` \
                 and include it as `tmux_pane` in the YAML frontmatter of the plan content."
                    .to_string(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

pub fn run_mcp_server(todotxt_dir: &str, todo_file: &str) -> Result<(), Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let service = TorudoMcpServer::new(todotxt_dir.to_string(), todo_file.to_string())
            .serve(stdio())
            .await?;
        service.waiting().await?;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_list_todos_tool() {
        let temp_dir = tempfile::tempdir().unwrap();
        let todo_file = temp_dir.path().join("todo.txt");
        let content = "(A) Fix bug +torudo @coding id:abc-123\n(B) Add feature +other @work id:def-456\n";
        fs::write(&todo_file, content).unwrap();

        let server = TorudoMcpServer::new(
            temp_dir.path().to_str().unwrap().to_string(),
            todo_file.to_str().unwrap().to_string(),
        );

        let params = ListTodosParams { project: None };
        let result = server
            .list_todos(rmcp::handler::server::wrapper::Parameters(params))
            .unwrap();

        let text = result
            .content
            .first()
            .and_then(|c| c.as_text())
            .map(|t| t.text.as_str())
            .unwrap_or("");
        let items: Vec<serde_json::Value> = serde_json::from_str(text).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["id"], "abc-123");
        assert_eq!(items[0]["description"], "Fix bug");
        assert_eq!(items[0]["priority"], "A");
        assert_eq!(items[1]["id"], "def-456");
    }

    #[test]
    fn test_list_todos_tool_with_project_filter() {
        let temp_dir = tempfile::tempdir().unwrap();
        let todo_file = temp_dir.path().join("todo.txt");
        let content = "(A) Fix bug +torudo @coding id:abc-123\n(B) Add feature +other @work id:def-456\n";
        fs::write(&todo_file, content).unwrap();

        let server = TorudoMcpServer::new(
            temp_dir.path().to_str().unwrap().to_string(),
            todo_file.to_str().unwrap().to_string(),
        );

        let params = ListTodosParams {
            project: Some("torudo".to_string()),
        };
        let result = server
            .list_todos(rmcp::handler::server::wrapper::Parameters(params))
            .unwrap();

        let text = result
            .content
            .first()
            .and_then(|c| c.as_text())
            .map(|t| t.text.as_str())
            .unwrap_or("");
        let items: Vec<serde_json::Value> = serde_json::from_str(text).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["id"], "abc-123");
    }

    #[test]
    fn test_register_plan_tool() {
        let temp_dir = tempfile::tempdir().unwrap();
        let todo_file = temp_dir.path().join("todo.txt");
        fs::write(&todo_file, "").unwrap();

        let server = TorudoMcpServer::new(
            temp_dir.path().to_str().unwrap().to_string(),
            todo_file.to_str().unwrap().to_string(),
        );

        let params = RegisterPlanParams {
            description: "Implement authentication".to_string(),
            plan: "# Auth Plan\n\n- Add login endpoint\n- Add JWT tokens".to_string(),
            project: "myapp".to_string(),
            priority: Some("A".to_string()),
        };
        let result = server
            .register_plan(rmcp::handler::server::wrapper::Parameters(params))
            .unwrap();

        let text = result
            .content
            .first()
            .and_then(|c| c.as_text())
            .map(|t| t.text.as_str())
            .unwrap_or("");
        let item: serde_json::Value = serde_json::from_str(text).unwrap();
        assert_eq!(item["description"], "Implement authentication");
        assert_eq!(item["priority"], "A");
        assert_eq!(item["projects"][0], "myapp");
        assert!(!item["id"].is_null());

        // Verify todo.txt was written
        let todo_content = fs::read_to_string(&todo_file).unwrap();
        assert!(todo_content.contains("Implement authentication"));
        assert!(todo_content.contains("+myapp"));
        assert!(todo_content.contains("(A)"));

        // Verify detail md was created
        let uuid = item["id"].as_str().unwrap();
        let md_path = temp_dir.path().join("todos").join(format!("{uuid}.md"));
        assert!(md_path.exists());
        let md_content = fs::read_to_string(&md_path).unwrap();
        assert!(md_content.contains("# Auth Plan"));
    }

    #[test]
    fn test_update_plan_tool() {
        let temp_dir = tempfile::tempdir().unwrap();
        let todo_file = temp_dir.path().join("todo.txt");
        fs::write(&todo_file, "").unwrap();

        let server = TorudoMcpServer::new(
            temp_dir.path().to_str().unwrap().to_string(),
            todo_file.to_str().unwrap().to_string(),
        );

        // First register a plan
        let register_params = RegisterPlanParams {
            description: "Build API".to_string(),
            plan: "# Original Plan\n\n- Step 1".to_string(),
            project: "myapp".to_string(),
            priority: None,
        };
        let register_result = server
            .register_plan(rmcp::handler::server::wrapper::Parameters(register_params))
            .unwrap();

        let text = register_result
            .content
            .first()
            .and_then(|c| c.as_text())
            .map(|t| t.text.as_str())
            .unwrap_or("");
        let item: serde_json::Value = serde_json::from_str(text).unwrap();
        let id = item["id"].as_str().unwrap().to_string();

        // Now update the plan
        let update_params = UpdatePlanParams {
            id: id.clone(),
            plan: "# Updated Plan\n\n- Step 1\n- Step 2".to_string(),
        };
        let update_result = server
            .update_plan(rmcp::handler::server::wrapper::Parameters(update_params))
            .unwrap();

        let update_text = update_result
            .content
            .first()
            .and_then(|c| c.as_text())
            .map(|t| t.text.as_str())
            .unwrap_or("");
        let update_json: serde_json::Value = serde_json::from_str(update_text).unwrap();
        assert_eq!(update_json["id"], id);

        // Verify the file was updated
        let md_path = temp_dir.path().join("todos").join(format!("{id}.md"));
        let md_content = fs::read_to_string(&md_path).unwrap();
        assert_eq!(md_content, "# Updated Plan\n\n- Step 1\n- Step 2");
    }
}
