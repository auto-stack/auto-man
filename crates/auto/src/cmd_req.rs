//! PLAN-014 T-03：`auto serve`/`auto req` 命令编排（自 main.rs 原样收编）。
//! 职责：AutoVM daemon 启动（前台/stdio/detach 经既有 auto_tool_execution
//! spawn_detached，不造第二 owned 设施）与 daemon 客户端请求面。
//! 行为合同：管道名/会话语义/输出 JSON 与文本形状零改。
use auto_lang_core::autovm_client::AutovmClient;

/// Serve 臂参数（clap 变体字段一一对应）。
pub(crate) struct ServeArgs {
    pub(crate) foreground: bool,
    pub(crate) stdio: bool,
    pub(crate) pipe_name: String,
    pub(crate) max_sessions: usize,
    pub(crate) timeout: u64,
}

pub(crate) fn serve(a: ServeArgs) -> miette::Result<()> {
    let ServeArgs {
        foreground,
        stdio,
        pipe_name,
        max_sessions,
        timeout,
    } = a;
let mut daemon =
    auto_lang_core::autovm_daemon::AutovmDaemon::new_with_config(max_sessions, timeout);
if stdio {
    daemon.run_stdio();
} else if foreground {
    daemon.run_pipe(&pipe_name);
} else {
    // Background mode: spawn self with --foreground to listen on named pipe.
    // PLAN-816 T-06：daemon 化 spawn 走设施 spawn_detached（分离
    // 语义=不持有归属，CREATE_NO_WINDOW 经 windows_creation_flags
    // 透传；协议/管道语义不动——契约基线 §3）。
    let exe = std::env::current_exe()
        .map_err(|e| miette::miette!("Cannot find auto executable: {}", e))?;
    let mut spec = auto_tool_execution::ExecutionSpec::new([
        exe.as_os_str().to_os_string(),
        "serve".into(),
        "--foreground".into(),
        "--pipe-name".into(),
        pipe_name.clone().into(),
    ]);
    spec.stdin = auto_tool_execution::StdioPolicy::Inherit;
    spec.stdout = auto_tool_execution::StdioPolicy::Inherit;
    spec.stderr = auto_tool_execution::StdioPolicy::Inherit;
    // F-816-B-R1-03：创建标志按平台消费——Windows 保持
    // CREATE_NO_WINDOW（旧行为），Unix 分离 spawn 不设标志
    //（旧代码在 Unix 本就不传 creation_flags；设施对非
    // Windows 非零值保持 InvalidSpec 负控）。
    #[cfg(windows)]
    {
        spec.windows_creation_flags = 0x0800_0000; // CREATE_NO_WINDOW
    }
    auto_tool_execution::spawn_detached(&spec)
        .map_err(|e| miette::miette!("Failed to start daemon: {}", e))?;
    println!("AutoVM daemon started (pipe: {})", pipe_name);
}
    Ok(())
}

/// Req 臂参数（clap 变体字段一一对应）。
pub(crate) struct ReqArgs {
    pub(crate) session: Option<String>,
    pub(crate) pipe_name: String,
    pub(crate) new_session: bool,
    pub(crate) inspect: bool,
    pub(crate) reset: bool,
    pub(crate) delete: bool,
    pub(crate) snapshot: bool,
    pub(crate) list: bool,
    pub(crate) json: bool,
    pub(crate) code: Option<String>,
}

pub(crate) fn req(a: ReqArgs) -> miette::Result<()> {
    let ReqArgs {
        session,
        pipe_name,
        new_session,
        inspect,
        reset,
        delete,
        snapshot,
        list,
        json,
        code,
    } = a;

let mut client =
    AutovmClient::connect(&pipe_name).map_err(|e| miette::miette!("{}", e))?;

if list {
    let resp = client.list().map_err(|e| miette::miette!("{}", e))?;
    if json {
        println!("{}", serde_json::to_string(&resp).unwrap());
    } else {
        match &resp.data {
            Some(d) => println!("Sessions: {}", d),
            None => println!("No sessions"),
        }
    }
    return Ok(());
}

if new_session {
    let ses_id = client.new_session().map_err(|e| miette::miette!("{}", e))?;
    println!("{}", ses_id);
    return Ok(());
}

let ses_id = match &session {
    Some(s) => s.clone(),
    None => {
        // Anonymous mode: create temp session, eval, delete
        if let Some(code) = &code {
            let (value, ok) = auto_lang_core::autovm_client::eval_one_shot(code);
            if json {
                let status = if ok { "ok" } else { "error" };
                println!("{}", serde_json::json!({"status": status, "value": value}));
            } else {
                if ok {
                    println!("{}", value);
                } else {
                    eprintln!("{}", value);
                }
            }
            return Ok(());
        }
        eprintln!("Error: provide --session <id>, --new-session, or code to evaluate");
        std::process::exit(1);
    }
};

if inspect {
    let resp = client
        .inspect(&ses_id)
        .map_err(|e| miette::miette!("{}", e))?;
    if json {
        println!("{}", serde_json::to_string(&resp).unwrap());
    } else {
        if resp.status == "ok" {
            if let Some(data) = &resp.data {
                println!("{}", serde_json::to_string_pretty(data).unwrap());
            }
        } else {
            eprintln!("Error: {}", resp.message.unwrap_or_default());
        }
    }
} else if reset {
    let resp = client
        .reset(&ses_id)
        .map_err(|e| miette::miette!("{}", e))?;
    if json {
        println!("{}", serde_json::to_string(&resp).unwrap());
    } else {
        println!("Session {} reset", ses_id);
    }
} else if delete {
    let resp = client
        .delete(&ses_id)
        .map_err(|e| miette::miette!("{}", e))?;
    if json {
        println!("{}", serde_json::to_string(&resp).unwrap());
    } else {
        println!("Session {} deleted", ses_id);
    }
} else if snapshot {
    let resp = client
        .snapshot(&ses_id)
        .map_err(|e| miette::miette!("{}", e))?;
    if json {
        println!("{}", serde_json::to_string(&resp).unwrap());
    } else {
        match &resp.value {
            Some(src) => println!("{}", src),
            None => eprintln!("Error: {}", resp.message.unwrap_or_default()),
        }
    }
} else if let Some(code) = &code {
    let resp = client
        .eval(&ses_id, code)
        .map_err(|e| miette::miette!("{}", e))?;
    if json {
        println!("{}", serde_json::to_string(&resp).unwrap());
    } else {
        if resp.status == "ok" {
            if let Some(val) = &resp.value {
                println!("{}", val);
            }
        } else {
            eprintln!("Error: {}", resp.message.unwrap_or_default());
        }
    }
} else {
    eprintln!("Error: provide code to evaluate or an action flag (--inspect, --reset, --delete, --snapshot)");
    std::process::exit(1);
}
    Ok(())
}
