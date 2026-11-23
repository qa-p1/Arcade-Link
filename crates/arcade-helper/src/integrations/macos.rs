//! Finder (PLAN §4.5): one Quick Action per entry ("Arcade…" and each pinned action), per user in
//! `~/Library/Services`. Finder lists them under Quick Actions (and Services) in a file's context menu, filtered to
//! the entry's file types.
//!
//! Each Quick Action is an Automator workflow bundle whose one step runs the helper with the selected files as
//! arguments, the way the Linux scripts do. Finder starts it once with the whole selection.

use std::path::{Path, PathBuf};

use super::linux::sh_quote;
use super::{Entry, File, Home, Plan};
use crate::mime;

/// The bundle name for an entry: `dev.arcade.helper.workflow` for the picker, `dev.arcade.helper.pin<n>.workflow`.
fn bundle_name(index: usize, entry: &Entry) -> String {
    if entry.is_picker() {
        "dev.arcade.helper.workflow".into()
    } else {
        format!("dev.arcade.helper.pin{index}.workflow")
    }
}

pub fn services_dir(home: &Home) -> PathBuf {
    home.home.join("Library").join("Services")
}

/// The Uniform Type Identifiers Finder matches an entry against. Broad system types per kind (each one Finder
/// knows on every supported macOS); a kind without one takes any file (`public.data`), and the helper still
/// checks each file before running.
pub fn file_types(entry: &Entry) -> Vec<&'static str> {
    if entry.is_picker() {
        return vec!["public.item"];
    }
    let mut out: Vec<&'static str> = Vec::new();
    let mut push = |t: &'static str| {
        if !out.contains(&t) {
            out.push(t);
        }
    };
    if entry.accepts.iter().any(|a| a == "folder/reference") {
        push("public.folder");
    }
    if mime::takes_any_file(&entry.accepts) {
        push("public.data");
        return out;
    }
    for accept in &entry.accepts {
        for kind in mime::kinds_for(accept) {
            push(match kind {
                "image" => "public.image",
                "svg" => "public.svg-image",
                "video" => "public.movie",
                "audio" => "public.audio",
                "pdf" => "com.adobe.pdf",
                "archive" => "public.archive",
                "text" | "markdown" => "public.plain-text",
                "json" => "public.json",
                "html" => "public.html",
                "code" => "public.source-code",
                "csv" => "public.comma-separated-values-text",
                "spreadsheet" => "public.spreadsheet",
                "presentation" => "public.presentation",
                "font" => "public.font",
                _ => "public.data",
            });
        }
    }
    out
}

/// What the workflow's shell step runs: the helper, the entry's arguments, then the selected files (`"$@"`).
pub fn command(exe: &Path, entry: &Entry) -> String {
    let mut parts = vec![sh_quote(&exe.to_string_lossy())];
    parts.extend(entry.args().iter().map(|a| sh_quote(a)));
    parts.push("\"$@\"".into());
    // Detached, so Finder's busy gear stops at once; the helper shows its own progress.
    format!("{} >/dev/null 2>&1 &", parts.join(" "))
}

fn xml(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

const PLIST_HEAD: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\">\n";

/// `Contents/Info.plist`: the Service Finder shows (title, file types, Finder only).
pub fn info_plist(entry: &Entry) -> String {
    let types: String = file_types(entry).iter().map(|t| format!("\t\t\t\t<string>{t}</string>\n")).collect();
    format!(
        "{PLIST_HEAD}<dict>\n\t<key>NSServices</key>\n\t<array>\n\t\t<dict>\n\t\t\t<key>NSMenuItem</key>\n\t\t\t<dict>\n\t\t\t\t<key>default</key>\n\t\t\t\t<string>{title}</string>\n\t\t\t</dict>\n\t\t\t<key>NSMessage</key>\n\t\t\t<string>runWorkflowAsService</string>\n\t\t\t<key>NSRequiredContext</key>\n\t\t\t<dict>\n\t\t\t\t<key>NSApplicationIdentifier</key>\n\t\t\t\t<string>com.apple.finder</string>\n\t\t\t</dict>\n\t\t\t<key>NSSendFileTypes</key>\n\t\t\t<array>\n{types}\t\t\t</array>\n\t\t</dict>\n\t</array>\n</dict>\n</plist>\n",
        title = xml(&entry.title),
    )
}

/// `Contents/document.wflow`: a Quick Action for Finder's files and folders with one "Run Shell Script" step that
/// takes them as arguments.
pub fn document(exe: &Path, entry: &Entry) -> String {
    format!(
        r#"{PLIST_HEAD}<dict>
	<key>AMApplicationBuild</key>
	<string>523</string>
	<key>AMApplicationVersion</key>
	<string>2.10</string>
	<key>AMDocumentVersion</key>
	<string>2</string>
	<key>actions</key>
	<array>
		<dict>
			<key>action</key>
			<dict>
				<key>AMAccepts</key>
				<dict>
					<key>Container</key>
					<string>List</string>
					<key>Optional</key>
					<true/>
					<key>Types</key>
					<array>
						<string>com.apple.cocoa.string</string>
					</array>
				</dict>
				<key>AMActionVersion</key>
				<string>2.0.3</string>
				<key>AMApplication</key>
				<array>
					<string>Automator</string>
				</array>
				<key>AMParameterProperties</key>
				<dict>
					<key>COMMAND_STRING</key>
					<dict/>
					<key>CheckedForUserDefaultShell</key>
					<dict/>
					<key>inputMethod</key>
					<dict/>
					<key>shell</key>
					<dict/>
					<key>source</key>
					<dict/>
				</dict>
				<key>AMProvides</key>
				<dict>
					<key>Container</key>
					<string>List</string>
					<key>Types</key>
					<array>
						<string>com.apple.cocoa.string</string>
					</array>
				</dict>
				<key>ActionBundlePath</key>
				<string>/System/Library/Automator/Run Shell Script.action</string>
				<key>ActionName</key>
				<string>Run Shell Script</string>
				<key>ActionParameters</key>
				<dict>
					<key>COMMAND_STRING</key>
					<string>{command}</string>
					<key>CheckedForUserDefaultShell</key>
					<true/>
					<key>inputMethod</key>
					<integer>1</integer>
					<key>shell</key>
					<string>/bin/sh</string>
					<key>source</key>
					<string></string>
				</dict>
				<key>BundleIdentifier</key>
				<string>com.apple.RunShellScript</string>
				<key>CFBundleVersion</key>
				<string>2.0.3</string>
				<key>CanShowSelectedItemsWhenRun</key>
				<false/>
				<key>CanShowWhenRun</key>
				<true/>
				<key>Category</key>
				<array>
					<string>AMCategoryUtilities</string>
				</array>
				<key>Class Name</key>
				<string>RunShellScriptAction</string>
				<key>InputUUID</key>
				<string>6F1D0D52-3B0C-4C3E-9C38-7D1A6C1E0001</string>
				<key>Keywords</key>
				<array>
					<string>Shell</string>
					<string>Script</string>
				</array>
				<key>OutputUUID</key>
				<string>6F1D0D52-3B0C-4C3E-9C38-7D1A6C1E0002</string>
				<key>UUID</key>
				<string>6F1D0D52-3B0C-4C3E-9C38-7D1A6C1E0003</string>
				<key>UnlocalizedApplications</key>
				<array>
					<string>Automator</string>
				</array>
				<key>arguments</key>
				<dict/>
				<key>isViewVisible</key>
				<integer>1</integer>
			</dict>
			<key>isViewVisible</key>
			<integer>1</integer>
		</dict>
	</array>
	<key>connectors</key>
	<dict/>
	<key>workflowMetaData</key>
	<dict>
		<key>applicationBundleIDsByPath</key>
		<dict/>
		<key>applicationPaths</key>
		<array/>
		<key>inputTypeIdentifier</key>
		<string>com.apple.Automator.fileSystemObject</string>
		<key>outputTypeIdentifier</key>
		<string>com.apple.Automator.nothing</string>
		<key>presentationMode</key>
		<integer>15</integer>
		<key>processesInput</key>
		<false/>
		<key>serviceApplicationBundleID</key>
		<string>com.apple.finder</string>
		<key>serviceApplicationPath</key>
		<string>/System/Library/CoreServices/Finder.app</string>
		<key>serviceInputTypeIdentifier</key>
		<string>com.apple.Automator.fileSystemObject</string>
		<key>serviceOutputTypeIdentifier</key>
		<string>com.apple.Automator.nothing</string>
		<key>serviceProcessesInput</key>
		<false/>
		<key>systemImageName</key>
		<string>NSActionTemplate</string>
		<key>useAutomaticInputType</key>
		<false/>
		<key>workflowTypeIdentifier</key>
		<string>com.apple.Automator.servicesMenu</string>
	</dict>
</dict>
</plist>
"#,
        command = xml(&command(exe, entry)),
    )
}

pub fn plan(exe: &Path, entries: &[Entry], home: &Home) -> Plan {
    let mut plan = Plan::default();
    let dir = services_dir(home);
    for (i, entry) in entries.iter().enumerate() {
        let contents = dir.join(bundle_name(i, entry)).join("Contents");
        plan.files.push(File { path: contents.join("Info.plist"), contents: info_plist(entry), executable: false });
        plan.files.push(File { path: contents.join("document.wflow"), contents: document(exe, entry), executable: false });
    }
    plan
}

/// Asks macOS to re-read the Services (Finder would on its own, but only after a while). A no-op elsewhere.
pub fn refresh() {
    #[cfg(target_os = "macos")]
    {
        let pbs = Path::new("/System/Library/CoreServices/pbs");
        if pbs.exists() {
            let _ = std::process::Command::new(pbs).arg("-update").stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pin() -> Entry {
        Entry { reference: "arcade.box/box:arcade.image.convert#webp".into(), title: "Convert to \"WebP\" & <more>".into(), accepts: vec!["file/image".into()] }
    }

    #[test]
    fn one_quick_action_per_entry_for_its_types() {
        let plan = plan(Path::new("/Applications/Arcade Tools.app/Contents/MacOS/arcade-helper"), &[Entry::picker(), pin()], &Home::under(Path::new("/h")));
        let paths: Vec<_> = plan.files.iter().map(|f| f.path.to_string_lossy().into_owned()).collect();
        assert_eq!(
            paths,
            [
                "/h/Library/Services/dev.arcade.helper.workflow/Contents/Info.plist",
                "/h/Library/Services/dev.arcade.helper.workflow/Contents/document.wflow",
                "/h/Library/Services/dev.arcade.helper.pin1.workflow/Contents/Info.plist",
                "/h/Library/Services/dev.arcade.helper.pin1.workflow/Contents/document.wflow",
            ]
        );
        assert!(plan.files[0].contents.contains("<string>public.item</string>"));
        let info = &plan.files[2].contents;
        assert!(info.contains("<string>Convert to &quot;WebP&quot; &amp; &lt;more&gt;</string>"), "{info}");
        assert!(info.contains("<string>public.image</string>") && info.contains("<string>public.svg-image</string>"), "{info}");
        assert!(!info.contains("public.item"));
        let doc = &plan.files[3].contents;
        assert!(
            doc.contains("<string>'/Applications/Arcade Tools.app/Contents/MacOS/arcade-helper' 'run' 'arcade.box/box:arcade.image.convert#webp' &quot;$@&quot; &gt;/dev/null 2&gt;&amp;1 &amp;</string>"),
            "{doc}"
        );
    }

    #[test]
    fn folders_and_any_file() {
        let e = Entry { reference: "a/b".into(), title: "t".into(), accepts: vec!["folder/reference".into(), "file/*".into()] };
        assert_eq!(file_types(&e), ["public.folder", "public.data"]);
        let e = Entry { reference: "a/b".into(), title: "t".into(), accepts: vec!["file/subtitle".into(), "file/pdf[]".into()] };
        assert_eq!(file_types(&e), ["public.data", "com.adobe.pdf"]);
    }
}
