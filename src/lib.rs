use std::sync::atomic::AtomicBool;

use colored::Colorize; //引入 colored 库以便在终端输出彩色文本
use regex::{Regex, RegexBuilder}; //引入 regex 库以便使用正则表达式进行更复杂的搜索
use std::error::Error;
use std::fs::{self};
use std::sync::{Arc, Mutex};
use std::time::Instant; //引入计时器

//use std::io::{self,BufRead,BufReader,Write};
use ignore::WalkBuilder;

use std::path::{Path, PathBuf}; //引入 Pathbuf 记录绝对路径

use clap::Parser; //引入 clap
//1.定义命令行参数结构体
#[derive(Parser, Debug)]
#[command(author="jiangxun",version="1.0",about="A mini grep tool with multithreading and colored output",long_about = None)]
struct Args {
    ///查询字符串
    #[arg(required = true)]
    query: String,
    ///文件路径
    #[arg(required = true)]
    file_path: String,
    ///忽略大小写
    #[arg(short = 'i', long)] //双引号意思是传入字符切片,单个字符传入的是单引号
    ignore_case: bool,
    ///显示统计信息,不显示匹配内容
    #[arg(short = 'l', long)]
    stats_only: bool,
}

struct FileStat {
    path: PathBuf,      //文件绝对路径
    match_count: usize, //该文件包含的匹配数量
}
pub struct Config {
    pub query: String,
    pub file_path: String,
    pub ignore_case: bool,
    pub stats_only: bool, //只有统计结果
}
impl Config {
    pub fn build() -> Result<Config, Box<dyn Error>> {
        let args = Args::parse(); //使用 clap 解析命令行参数

        // //返回值是 Result<Config,&'static str>类型
        // // &'static str表示字符串字面值在程序运行期间都是有效的
        // if args.len() <3 {
        //     return Err("Not enough arguments,please provide a query and a file path");
        // }
        // if args.len() >3{
        //     return Err("Too many arguments,please provide only a query and a file path");
        // }
        // let query = args[1].clone();
        // let file_path = args[2].clone();
        // let ignore_case = env::var("IGNORE_CASE").is_ok();//需要环境变量设置 IGNROE_CASE
        // //默认情况是区分大小写的搜索
        Ok(Config {
            query: args.query,
            file_path: args.file_path,
            ignore_case: args.ignore_case,
            stats_only: args.stats_only,
        })
    }
    pub fn run(&self) -> Result<(), Box<dyn Error>> //Box<dyn std::error::Error>表示函数可能返回任何类型的错误
    {
        //1.性能IO 优化
        //1.预先构建正则表达式
        //-----计时开始-------
        let start_time = Instant::now();

        let re = RegexBuilder::new(&regex::escape(&self.query))
            .case_insensitive(self.ignore_case) //忽略大小写
            .build()?;
        //2.使用 Arc 包装 Regex,以便在多线程共享
        let re = Arc::new(re);
        //3.创建一个输出锁,防止多线程打印是文字乱序
        let stdout_mutex = Arc::new(std::sync::Mutex::new(()));

        ////共享统计列表
        let stats = Arc::new(Mutex::new(Vec::<FileStat>::new())); //使用锁新建文件列表
        let found_any = Arc::new(AtomicBool::new(false));

        //4.构建并行便遍历(walkbuilder)
        let walker = WalkBuilder::new(&self.file_path) //使用 walkbuider 忽略隐藏文件和目录
            .threads(num_cpus::get()) //使用系统的 CPU 核心数作为线程数
            .build_parallel(); //构建并行遍历器
        //5.执行并行搜索
        walker.run(|| {
            //这个闭包会递归应用到每一个目录和文件
            let re_thread = Arc::clone(&re); //让多个线程共享正则表达式
            let stdout_mutex_thread = Arc::clone(&stdout_mutex);

            let found_any_thread = Arc::clone(&found_any);
            //克隆 stats 的引用给每个线程
            let stats_thread = Arc::clone(&stats);
            let stats_only_mode = self.stats_only;

            Box::new(move |result| {
                use ignore::WalkState; //引入 WalkState 枚举
                match result {
                    Ok(entry) => {
                        if entry.file_type().is_some_and(|ft| ft.is_file()) {
                            //调用搜索,并且传入 stats_thread

                            search_file(
                                entry.path(),
                                &re_thread,
                                &stdout_mutex_thread,
                                &found_any_thread,
                                &stats_thread,
                                stats_only_mode,
                            );
                        }
                        WalkState::Continue
                    }
                    Err(_err) => {
                        //  eprintln!("Error reading entry:{}",err);
                        WalkState::Continue
                    }
                }
            })
        });
        //     let file = File::open(&self.file_path)?;//?操作符会自动传播错误
        //     let reader = BufReader::new(file);//创建带缓冲的读取器,默认缓冲*8KB

        //    // let contents = std::fs::read_to_string(&self.file_path)?;
        //     search_and_print(&self.query,reader,self.ignore_case)?;//处理 result 结果

        ////---------------搜索结束,开始统计输出-----------------
        let duration = start_time.elapsed(); //获取总耗时
        let stats = stats.lock().unwrap(); //获取统计数据的锁
        //没找到匹配项就不打印统计信息
        if !found_any.load(std::sync::atomic::Ordering::Relaxed) {
            println!("{}", "No matches found.".yellow());
            return Ok(());
        }
        println!("\n{}", "--------Statistics---------".bold());
        let mut total_matches = 0;
        for stat in stats.iter() {
            //获取绝对路径字符串
            let path_str = stat.path.display().to_string();
            total_matches += stat.match_count;
            //格式化输出:[文件绝对路径]-[该文件匹配数]
            //路径显示蓝色, 匹配数粉色
            println!(
                "{} | Matches: {}",
                path_str.blue(),
                stat.match_count.to_string().magenta()
            );
        }
        println!("{}", "---------------------------".bold());
        //时间显示为紫色,总匹配数粉色
        println!("总计找到文件数目:{}", stats.len().to_string().cyan());
        println!("总计找到匹配项数:{}", total_matches.to_string().magenta());
        println!("耗时:{:.7}秒", duration.as_secs_f64().to_string().purple());

        Ok(())

        //执行搜索的逻辑
    }
}

fn search_file(
    path: &Path,
    re: &Regex,
    output_lock: &Arc<Mutex<()>>,
    found_signal: &AtomicBool,
    stats: &Arc<Mutex<Vec<FileStat>>>,
    stats_only: bool,
) {
    let contents = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_err) => {
            //eprintln!("Error reading file {}:{}",path.display(),err);
            return;
        }
    };
    let mut matched_lines = Vec::new();
    let mut file_match_count = 0;
    for (i, line) in contents.lines().enumerate() {
        if re.is_match(line) {
            file_match_count += 1;
            let line_number = i + 1;
            let line_colored =
                re.replace_all(line, |caps: &regex::Captures| caps[0].red().to_string()); //用红色的查询字符串替换行
            matched_lines.push((line_number, line_colored));
        }
    }
    //如果不是只统计模式,要进行 io 打印
    //这个 if 块会被直接跳过,因此会影响匹配信号的检索. found_signal
    if !stats_only && !matched_lines.is_empty() {
        //found_signal.store(true,std::sync::atomic::Ordering::Relaxed);//设置找到匹配项的信号
        let _lock = output_lock.lock().unwrap(); //锁定输出锁
        println!("File: {}", path.display().to_string().blue());
        for (line_number, line_colored) in matched_lines {
            println!("{} {}", format!("{}:", line_number).green(), line_colored);
        }
        println!();
    }
    if file_match_count > 0 {
        found_signal.store(true, std::sync::atomic::Ordering::Relaxed); //设置找到匹配项的信号
    }
    let abs_path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let stat_entry = FileStat {
        path: abs_path,
        match_count: file_match_count,
    };

    match stats.lock() {
        Ok(mut s) => s.push(stat_entry),
        Err(_e) => {
            //如果锁被其他线程持有,则跳过统计
        }
    }
}
// pub fn search_and_print<R:BufRead>(query:&str,mut reader:R,_ignore_case:bool) -> Result<(),Box<dyn std::error::Error>>{

//     let pattern = if _ignore_case {format!(r"(?i){}",regex::escape(&query))} else {regex::escape(&query)};
//     let re = Regex::new(&pattern)?;//re 是一个正则表达式对象
//     let mut line_buf = String::new();
//     let mut line_num = 1;
//     let mut found_any = false;

//     //2. 输出锁
//     let stdout = io::stdout();
//     let mut handle = stdout.lock();//锁定标准输出
//     //for (i,line) in contents.lines().enumerate(){}
//     while reader.read_line(&mut line_buf)? >0 {
//     if re.is_match(&line_buf){
//     found_any = true;
//     let line_number = format!("{}:",line_num).green();//行号从1开始
//     let line_colored = re.replace_all(&line_buf,|caps:&regex::Captures|{caps[0].red().to_string()});//用红色的查询字符串替换行中的原始查询字符串
//     //println!("{} {}",line_number,line_colored);
//     writeln!(handle,"{} {}",line_number,line_colored)?;//使用锁定的标准输出句柄写入
//     }
//     line_buf.clear();
//     line_num += 1;
// }
//     if !found_any{
//         println!("{}", "No matches found.".yellow());
//     }
//     // if re.find_iter(contents).next().is_none(){
//     //     println!("{}", "No matches found.".yellow());
//     // }//如果没有找到匹配项，打印提示信息
//     Ok(())
//     }

// pub fn case_insensitive_search<'a>(query:&str,contents:&'a str) -> Vec<(usize,&'a str)>{
//     let query = query.to_lowercase();
//     let mut results = Vec::new();
//     for (i,line) in contents.lines().enumerate(){
//         if line.to_lowercase().contains(&query){
//             results.push((i+1,line));
//         }
//     }
//     results
// }
// pub fn parse_config(args:&[String]) -> Config{
//     let query = args[1].clone();
//     let file_path = args[2].clone();
//     Config {query,file_path}
// }
