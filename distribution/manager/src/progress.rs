// SPDX-License-Identifier: GPL-3.0-or-later
use std::{io::{self,IsTerminal,Write},sync::{Arc,atomic::{AtomicBool,Ordering}},thread,time::{Duration,Instant}};
/// Indeterminate stages show activity and elapsed time; downloads use curl's real percentage.
pub fn run<T,E>(label:&str,operation:impl FnOnce()->Result<T,E>)->Result<T,E>{
    let start=Instant::now();let stop=Arc::new(AtomicBool::new(false));let signal=stop.clone();let name=label.to_owned();
    let terminal=io::stderr().is_terminal()||std::env::var("CLBB_PROGRESS_TTY").as_deref()==Ok("1");
    eprintln!("[CLBB …] {label}");
    let worker=thread::spawn(move||{
        let frames=["[>         ]","[=>        ]","[==>       ]","[===>      ]","[====>     ]","[=====>    ]","[======>   ]","[=======>  ]","[========> ]","[=========>]"];
        let mut tick=0;
        loop{thread::park_timeout(Duration::from_millis(120));if signal.load(Ordering::Acquire){break;}
            if terminal {eprint!("\r{} {} — {:.1}s",frames[tick%frames.len()],name,start.elapsed().as_secs_f32());let _=io::stderr().flush();}
            else if tick%25==24 {eprintln!("[CLBB …] {} — {}s",name,start.elapsed().as_secs());}
            tick+=1;
        }
    });
    let result=operation();stop.store(true,Ordering::Release);worker.thread().unpark();let _=worker.join();
    if terminal {eprint!("\r\x1b[2K");}eprintln!("[CLBB {}] {} ({:.2}s)",if result.is_ok(){"OK"}else{"ERROR"},label,start.elapsed().as_secs_f32());result
}
