//! Strict OPTIONAL tool executables for actual-binary external-viewer journeys.
//! Registered declarations only; no test/tool execution is claimed here.
use crate::support::{Fixture, Session};
use serde_json::{json, Value};
use std::{
    ffi::OsStr,
    fs,
    os::unix::fs::{symlink, PermissionsExt},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

#[derive(Clone, Copy)]
pub(super) enum RendererV4 {
    Ready,
    Missing,
    ExitTwo,
    Overflow,
    Hold,
}
#[derive(Clone, Copy)]
pub(super) enum PagerV4 {
    Ready,
    Unpatched,
    ExitTwo,
    Hostile,
    OverboundEdit,
}
pub(super) struct ExternalToolsV4 {
    path: PathBuf,
    journal: PathBuf,
    release: PathBuf,
}

fn installed(name: &str) -> PathBuf {
    for directory in std::env::split_paths(&std::env::var_os("PATH").unwrap()) {
        let Ok(path) = fs::canonicalize(directory.join(name)) else {
            continue;
        };
        let metadata = fs::metadata(&path).unwrap();
        if metadata.is_file() && metadata.permissions().mode() & 0o111 != 0 {
            return path;
        }
    }
    panic!("required existing test fixture executable {name} unavailable");
}
fn executable(directory: &Path, name: &str, node: &Path, source: &str) {
    assert!(node.is_absolute());
    assert!(!node.as_os_str().as_encoded_bytes().contains(&b'\n'));
    let path = directory.join(name);
    assert!(
        !path.exists(),
        "new task owns this exact fixture executable"
    );
    fs::write(&path, format!("#!{}\n{source}", node.display())).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}

impl ExternalToolsV4 {
    pub(super) fn install(fixture: &Fixture, renderer: RendererV4, pager: PagerV4) -> Self {
        let path = fixture.root.join("bin");
        // PATH is child-only and contains NO system fallback. A missing optional
        // executable is deterministic even on a host with delta/difft installed.
        let git = installed("git");
        let node = installed("node");
        let stty = installed("stty");
        assert!(!path.join("git").exists());
        symlink(git, path.join("git")).unwrap();
        let journal = fixture.root.join("external-tools.jsonl");
        let release = fixture.root.join("external-renderer-release");
        fs::write(&journal, b"").unwrap();
        let common = format!(
            "'use strict';\nconst fs=require('node:fs'),path=require('node:path'),cp=require('node:child_process');\nconst journal={},release={},stty={};\nfunction note(value){{fs.appendFileSync(journal,JSON.stringify(value)+'\\n')}}\nfunction fail(code){{process.exit(code)}}\nconst tool=path.basename(process.argv[1]),args=process.argv.slice(2);\nconst forbidden=['GH_TOKEN','GITHUB_TOKEN','GH_ENTERPRISE_TOKEN','GITHUB_ENTERPRISE_TOKEN','GH_CONFIG_DIR','GIT_DIR','GIT_WORK_TREE','LESSOPEN','LESSCLOSE','LESSKEY','LESS','LD_PRELOAD','DYLD_INSERT_LIBRARIES'];\nfor(const key of forbidden)if(process.env[key]!==undefined)fail(81);\nif(process.env.SHELL!=='/bin/false'||process.env.TERM!=='xterm-256color'||process.env.LESSSECURE!=='1'||process.env.LESSSECURE_ALLOW!=='')fail(82);\nif(process.env.HOME!==process.cwd()||process.env.XDG_CONFIG_HOME!==process.cwd()||process.env.TMPDIR!==process.cwd())fail(83);\n",
            json!(journal), json!(release), json!(stty)
        );
        if !matches!(renderer, RendererV4::Missing) {
            let outcome = match renderer {
                RendererV4::Ready => "ready",
                RendererV4::ExitTwo => "exit2",
                RendererV4::Overflow => "overflow",
                RendererV4::Hold => "hold",
                RendererV4::Missing => unreachable!(),
            };
            let renderer_source = format!(
                "{common}\nif(args.length===1&&args[0]==='--version'){{note({{tool,phase:'probe',args,pid:process.pid}});process.stdout.write(tool==='delta'?'delta 0.18.2\\n':'difftastic 0.66.0\\n');process.exit(0)}}\nlet input,oldBytes,newBytes;\nif(tool==='delta'){{if(args.length!==5||args[0]!=='--no-gitconfig'||args[1]!=='--paging=never'||args[2]!=='--detect-dark-light=never'||!/^--width=([1-9][0-9]*)$/.test(args[3])||args[4]!=='--line-numbers')fail(84);input=fs.readFileSync(0)}}\nelse if(tool==='difft'){{if(args.length!==11||JSON.stringify(args.slice(0,2))!==JSON.stringify(['--display=side-by-side-show-both','--color=always'])||!/^--width=([1-9][0-9]*)$/.test(args[2])||JSON.stringify(args.slice(3,9))!==JSON.stringify(['--context=3','--byte-limit=1048576','--graph-limit=250000','--parse-error-limit=0','--strip-cr=off','--']))fail(85);for(const file of args.slice(9)){{const s=fs.lstatSync(file);if(!s.isFile()||s.isSymbolicLink()||(s.mode&511)!==256||s.nlink!==1||path.dirname(file)!==process.cwd()||!/^(old|new)\\.[a-zA-Z0-9]{{1,12}}$/.test(path.basename(file)))fail(86)}}oldBytes=fs.readFileSync(args[9]);newBytes=fs.readFileSync(args[10])}}else fail(87);\nconst mode=fs.statSync(process.cwd()).mode&511;if(mode!==448)fail(88);\nnote({{tool,phase:'body',args,pid:process.pid,parent:process.ppid,cwd:process.cwd(),mode,input:input?.toString('hex'),old:oldBytes?.toString('hex'),new:newBytes?.toString('hex'),tty:!!process.stdout.isTTY}});\nconst outcome='{outcome}';\nfunction finish(){{if(outcome==='exit2'){{process.stderr.write('PRIVATE-CHILD-DIAGNOSTIC');process.exit(2)}}if(outcome==='overflow'){{process.stdout.write(Buffer.alloc(524289,120));return}}process.stdout.write('EXTERNAL ORIGINAL RENDER\\n');if(tool==='delta')process.stdout.write(input);else{{process.stdout.write('EXTERNAL OLD\\n');process.stdout.write(oldBytes);process.stdout.write('EXTERNAL NEW\\n');process.stdout.write(newBytes)}}}}\nif(outcome==='hold'){{const timer=setInterval(()=>{{if(fs.existsSync(release)){{clearInterval(timer);finish()}}}},10)}}else finish();\n"
            );
            executable(&path, "delta", &node, &renderer_source);
            executable(&path, "difft", &node, &renderer_source);
        }
        let pager_outcome = match pager {
            PagerV4::Ready => "ready",
            PagerV4::Unpatched => "old",
            PagerV4::ExitTwo => "exit2",
            PagerV4::Hostile => "hostile",
            PagerV4::OverboundEdit => "overbound",
        };
        let pager_source = format!(
            "{common}\nconst outcome='{pager_outcome}';\nif(args.length===1&&args[0]==='--version'){{note({{tool,phase:'probe',args,pid:process.pid}});process.stdout.write(outcome==='old'?'less 643\\n':'less 710\\n');process.exit(0)}}\nif(JSON.stringify(args.slice(0,4))!==JSON.stringify(['-R','-K','-L','--'])||args.length!==5)fail(89);\nif(!process.stdin.isTTY||!process.stdout.isTTY||!process.stderr.isTTY)fail(90);\nif(process.env.LESSHISTFILE!=='/dev/null'||process.env.LESSHISTSIZE!=='0'||process.env.LESSCHARSET!=='utf-8')fail(91);\nconst file=args[4],s=fs.lstatSync(file);if(!s.isFile()||s.isSymbolicLink()||(s.mode&511)!==256||s.nlink!==1||path.dirname(file)!==process.cwd()||(fs.statSync(process.cwd()).mode&511)!==448)fail(92);\nconst bytes=fs.readFileSync(file);\nconst initial=cp.spawnSync(stty,['-g'],{{stdio:[0,'pipe','pipe']}});if(initial.status!==0)fail(93);\nprocess.stdin.setRawMode(true);process.stdin.resume();\nconst raw=cp.spawnSync(stty,['-g'],{{stdio:[0,'pipe','pipe']}});if(raw.status!==0)fail(94);\nconst description=cp.spawnSync(stty,['-a'],{{stdio:[0,'pipe','pipe']}});if(description.status!==0)fail(95);\nnote({{tool,phase:'pty',args,pid:process.pid,parent:process.ppid,cwd:process.cwd(),file,input:bytes.toString('hex'),initialTermios:initial.stdout.toString(),rawTermios:raw.stdout.toString(),rawDescription:description.stdout.toString(),stdin:process.stdin.isTTY,stdout:process.stdout.isTTY,stderr:process.stderr.isTTY,columns:process.stdout.columns,rows:process.stdout.rows}});\nconst lines=bytes.toString('utf8').split('\\n');let first=0;function pageRows(){{return Math.max(1,process.stdout.rows-2)}}function draw(prefix){{process.stdout.write('\\x1b[2J\\x1b[H'+prefix+'\\n'+lines.slice(first,first+pageRows()).join('\\n'))}}\nif(outcome==='exit2'){{process.stderr.write('PRIVATE-PAGER-DIAGNOSTIC');process.exit(2)}}\nif(outcome==='overbound'){{process.stdout.write('\\x1b[32767@');setInterval(()=>{{}},1000)}}else{{draw('EXTERNAL PAGER READY');if(outcome==='hostile')process.stdout.write('\\x1b]0;PRIVATE-EXTERNAL-TITLE\\x07\\x1b]52;c;PRIVATE-CLIPBOARD\\x07\\x1b[?1003h\\x1b[?2004h');}}\nprocess.stdout.on('resize',()=>{{note({{tool,phase:'resize',pid:process.pid,columns:process.stdout.columns,rows:process.stdout.rows}});draw('EXTERNAL SIZE '+process.stdout.columns+'x'+process.stdout.rows)}});\nprocess.stdin.on('data',chunk=>{{note({{tool,phase:'key',pid:process.pid,hex:chunk.toString('hex')}});const key=chunk.toString();if(key==='j')first=Math.min(first+1,Math.max(0,lines.length-pageRows()));else if(key==='k')first=Math.max(0,first-1);else if(key===' ')first=Math.min(first+pageRows(),Math.max(0,lines.length-pageRows()));else if(key==='b')first=Math.max(0,first-pageRows());else if(key==='g')first=0;else if(key==='G')first=Math.max(0,lines.length-pageRows());draw('EXTERNAL KEY '+JSON.stringify(key))}});\n"
        );
        executable(&path, "less", &node, &pager_source);
        Self {
            path,
            journal,
            release,
        }
    }
    pub(super) fn path(&self) -> &OsStr {
        self.path.as_os_str()
    }
    pub(super) fn records(&self) -> Vec<Value> {
        fs::read_to_string(&self.journal)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).expect("actual strict executable record"))
            .collect()
    }
    pub(super) fn phase(&self, tool: &str, phase: &str) -> Vec<Value> {
        self.records()
            .into_iter()
            .filter(|row| row["tool"] == tool && row["phase"] == phase)
            .collect()
    }
    pub(super) fn wait_phase(
        &self,
        session: &mut Session,
        tool: &str,
        phase: &str,
        count: usize,
    ) -> Vec<Value> {
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            let rows = self.phase(tool, phase);
            if rows.len() == count {
                return rows;
            }
            assert!(rows.len() < count, "no extra optional tool execution");
            session.poll_output(Duration::from_millis(1));
            assert!(
                Instant::now() < deadline,
                "actual {tool} {phase} record missing"
            );
        }
    }
    pub(super) fn release(&self) {
        fs::write(&self.release, b"release original held renderer\n").unwrap();
    }
}
