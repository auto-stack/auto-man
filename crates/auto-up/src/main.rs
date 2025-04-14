use auto_lang::{AutoResult, config::AutoConfig};
use auto_val::{AutoStr, Obj};
use colored::Colorize;
use dialoguer::{Input, Select, console::Style, theme::ColorfulTheme};
use reqwest::blocking::get;
use std::fs::{self, File};
use std::path::Path;
use std::sync::OnceLock;
use std::{cmp::Ordering, env};
use version_compare::{CompOp, VersionCompare};
use zip::ZipArchive;

static MY_THEME: OnceLock<ColorfulTheme> = OnceLock::new();
fn select_tool() -> AutoResult<AutoStr> {
    // Implement logic to select a tool
    let tools = vec!["auto-man"]; //, "auto-lang", "auto-ui"];
    let selection = Select::with_theme(MY_THEME.get().unwrap())
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

    println!("checking releases from {}", url);

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
    let selection = Select::with_theme(MY_THEME.get().unwrap())
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
    let auto_dir = home.join(".auto");
    // if au.at not in dir, this is the first time to install
    let au_conf = auto_dir.join("auto-up.at");
    if au_conf.is_file() {
        // look for installation dir
        let conf = AutoConfig::from_file(&au_conf, &Obj::EMPTY)?;
        let install_dir = conf.root.props.get_str_of("auto-man");
        return Ok(install_dir);
    } else {
        // first time
        // let user input one dir
        let default_am_location = auto_dir.join(tool);
        println!();
        let input = Input::<String>::with_theme(MY_THEME.get().unwrap())
            .with_prompt("Enter installation directory")
            .with_initial_text(default_am_location.to_str().unwrap())
            .interact_text()?;

        // store this dir to auto-up.at
        fs::create_dir_all(default_am_location)?;
        let code = format!(r#""auto-man": "{}"\n"#, input);
        fs::write(au_conf, code)?;
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
    // download installation zip to temp dir
    let temp_dir = env::temp_dir();

    println!(
        "downloading {} to temp dir {}",
        info.name,
        temp_dir.display()
    );

    // download zip
    let zip_name = info.url.split('/').last().unwrap();
    let zip_path = temp_dir.join(zip_name);
    if zip_path.is_file() {
        println!("zip file already exists");
    } else {
        let resp = reqwest::blocking::get(info.url.as_str()).expect("request failed");
        let body = resp.bytes().expect("body invalid");
        std::fs::write(zip_path.clone(), &body)?;
    }

    let install_path = Path::new(install_dir.as_str());

    // unzip
    extract_zip(&zip_path, &info, &install_path)?;

    //
    println!("Installation directory: {}", install_dir);

    // 2. download selected release
    // 3. install selected release
    Ok(())
}

fn get_simple_name(name: &AutoStr) -> AutoStr {
    match name.as_str() {
        "auto-man" => "am".into(),
        "auto-gen" => "ag".into(),
        _ => name.clone(),
    }
}

fn extract_zip(zip_path: &Path, info: &ReleaseInfo, install_dir: &Path) -> AutoResult<()> {
    std::fs::create_dir_all(install_dir)?;
    let mut zip = ZipArchive::new(File::open(zip_path)?)?;
    for i in 0..zip.len() {
        let mut file = zip.by_index(i)?;
        let basename = file.name();
        let name = info.name.to_string();
        let path = install_dir.join(name.clone() + "_" + info.version.as_str() + ".exe");
        let simple_name = get_simple_name(&info.name);
        let link_path = install_dir.join(name.clone() + ".exe");
        let simple_link_path = install_dir.join(simple_name.to_string() + ".exe");
        if basename != name + ".exe" {
            continue;
        }
        println!("extracting {}", basename);
        if file.is_dir() {
            std::fs::create_dir_all(&path)?;
        } else {
            let mut out = File::create(&path)?;
            std::io::copy(&mut file, &mut out)?;
            println!("{}", format!("installed {}", path.display()).bright_blue());
            use std::os::windows;
            if link_path.is_symlink() {
                std::fs::remove_file(&link_path)?;
            }
            windows::fs::symlink_file(&path, &link_path)?;
            println!(
                "{}",
                format!("symlinked {}", link_path.display()).bright_blue()
            );
            if simple_link_path.is_symlink() {
                std::fs::remove_file(&simple_link_path)?;
            }
            windows::fs::symlink_file(&path, &simple_link_path)?;
            println!(
                "{}",
                format!("symlinked {}", simple_link_path.display()).bright_blue()
            );
        }
    }
    Ok(())
}

fn main() -> AutoResult<()> {
    std::thread::spawn(|| {
        MY_THEME.get_or_init(|| ColorfulTheme {
            prompt_style: Style::default().bold().yellow().bright(),
            ..Default::default()
        });
    })
    .join()
    .unwrap();

    auto_up()?;
    Ok(())
}
