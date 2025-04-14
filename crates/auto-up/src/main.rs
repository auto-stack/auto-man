use auto_lang::AutoResult;
use auto_val::AutoStr;
use dialoguer::Select;

fn select_tool() -> AutoResult<AutoStr> {
    // Implement logic to select a tool
    let tools = vec!["auto-man"]; //, "auto-lang", "auto-ui"];
    let selection = Select::new()
        .with_prompt("-- Which tool do you want to upgrade?")
        .default(0)
        .items(&tools)
        .interact()?;
    Ok(tools[selection].into())
}

fn list_releases() {
    // Implement logic to list all releases and let user select one
}

fn auto_up() -> AutoResult<()> {
    println!("Welcome to auto-up, a tool to upgrade auto-lang related tools");
    println!();
    let tool = select_tool()?;
    println!("Selected tool: {}", tool);
    println!();
    // 1. list all releases and let user select one
    list_releases();
    // 2. download selected release
    // 3. install selected release
    Ok(())
}

fn main() -> AutoResult<()> {
    auto_up()?;
    Ok(())
}
