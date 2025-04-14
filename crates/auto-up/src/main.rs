use auto_lang::{AutoResult, config::AutoConfig};
use auto_val::{AutoStr, Obj};
use colored::Colorize;
use dialoguer::{Input, Select, console::Style, theme::ColorfulTheme};
use reqwest::blocking::get;
use std::cmp::Ordering;
use version_compare::{CompOp, VersionCompare};

fn select_tool() -> AutoResult<AutoStr> {
    // Implement logic to select a tool
    let tools = vec!["auto-man"]; //, "auto-lang", "auto-ui"];
    let mut theme = ColorfulTheme::default();
    theme.prompt_style = Style::default().bold().yellow().bright();
    let selection = Select::with_theme(&theme)
        .with_prompt("Which tool do you want to install?")
        .default(0)
        .items(&tools)
        .interact()?;
    Ok(tools[selection].into())
}

pub struct ReleaseInfo {
    pub name: AutoStr,
    pub version: AutoStr,
    pub url: AutoStr,
}

fn list_releases(name: &AutoStr) -> AutoResult<ReleaseInfo> {
    let url = format!(
        "https://gitee.com/auto-stack/auto-man/raw/master/crates/auto-up/assets/{}.at",
        name
    );

    let page = get(url)?.text()?;

    let config = AutoConfig::from_code(page, &Obj::EMPTY)?;

    let releases = config.root.get_nodes("release");

    let mut versions = vec![];
    for r in releases.iter() {
        println!("Release: {}", r);
        let v = r.props.get_str_or("version", "unknown");
        versions.push(v);
    }
    versions.sort_by(|a, b| match VersionCompare::compare(a, b) {
        Ok(CompOp::Eq) => Ordering::Equal,
        Ok(CompOp::Lt) => Ordering::Less,
        Ok(CompOp::Gt) => Ordering::Greater,
        Err(_) => Ordering::Equal,
        _ => Ordering::Equal,
    });

    // select one version
    //
    println!();
    let mut theme = ColorfulTheme::default();
    theme.prompt_style = Style::default().bold().yellow().bright();
    let selection = Select::with_theme(&theme)
        .with_prompt("Which version do you want to install?")
        .default(0)
        .items(&versions)
        .interact()?;

    let version = versions[selection].clone();
    println!("Selected version: {}", version);

    let url = &releases[selection].props.get_str_or("url", "unknown");

    if url == "unknown" {
        println!("No URL found for the selected version: {}", version);
    }

    println!("downloading {}", url);

    Ok(ReleaseInfo {
        name: name.clone(),
        version,
        url: url.into(),
    })
}

fn get_install_dir(tool: &str) -> AutoResult<AutoStr> {
    let home = dirs::home_dir().ok_or("Can't open home dir")?;
    let dir = home.join(".auto");
    // if au.at not in dir, this is the first time to install
    let au_conf = dir.join("auto-up.at");
    if au_conf.is_file() {
        // look for installation dir
        let conf = AutoConfig::from_file(&au_conf, &Obj::EMPTY);
        let Ok(conf) = conf else {
            return Err("Failed to parse auto-up.at".into());
        };
        let install_dir = conf.root.props.get_str_of("auto-man");
        return Ok(install_dir);
    } else {
        // first time
        // let user input one dir
        let mut theme = ColorfulTheme::default();
        theme.prompt_style = Style::default().bold().yellow().bright();
        let input = Input::<String>::with_theme(&theme)
            .with_prompt("Enter installation directory")
            .interact_text()?;
        return Ok(input.into());
    }
}

fn auto_up() -> AutoResult<()> {
    println!(
        "{}",
        "Welcome to auto-up, a tool to upgrade auto-lang related tools"
            .bright_yellow()
            .bold()
    );
    println!();
    let tool = select_tool()?;
    println!("Selected tool: {}", tool);
    println!();
    // 1. list all releases and let user select one
    let info = list_releases(&tool)?;
    // check installation directory
    let install_dir = get_install_dir(&tool)?;

    println!("Installation directory: {}", install_dir);

    // 2. download selected release
    // 3. install selected release
    Ok(())
}

fn main() -> AutoResult<()> {
    auto_up()?;
    Ok(())
}
