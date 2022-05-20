# Binaries

一个二进制程序管理下载器

## 目的

由于golang,rust等静态语言的流行，许多程序只有单个文件，不需要外部依赖执行。同时由于linux等包管理器的更新滞后，无法更新到最新版本

如何管理这些程序就是一个问题

当前在github上可能找到的能管理bin的有zinit，但同时这又是一个zsh插件管理器，很难抉择。另外zinit不稳定（原作者删库），更新也将不再活跃。从性能的方面而言，下载时无法并行安装，速度较慢。所以不是一个好的选择。

另外有些如BinMan等可以下载，甚至github官方cli也能下载`gh release
download`，但无法做到自动化，如多平台的安装

在发现这些问题后，管理程序应该要做到自动配置安装bin，有以下功能：

* 配置文件定义要管理的bins
* 下载时可以自动根据平台选择或可配置自动选择
* 有基本的安装更新卸载功能
* 快速下载安装

## 实现

### 自动选择下载

这个功能相对是比较复杂的，因为github
release时不同的仓库可能会有不同的命名方式，虽然基本是`bin_name-os-arch.mime`。幸好zinit有对应的实现，减少了许多问题

正则可以很好的使用一行配置多平台，主要的选择算法如下:

* 如果用户有配置则使用配置选择，否则
* 对assets中的name根据平台os,arch,bin-name过滤，如果无法找到assets退出，否则
* 根据文件content type与支持的类型过滤（如解压），如果找到多个则
* 根据download counts找最多下载的尽力找出合适的并给出警告

在自动选择不合适的情况下，用户的配置使用正则可以很好的解决多平台选择的问题：`(amd64|arm64)`

### 选择bin

在github上的release中大多都是压缩后的asset，此时应该有解压程序，

* 用户配置的解压hook存在时，直接使用hook解压。解压后在目录中找bin_name对应的文件夹
* 内置支持zip,gz解压，如果无法解压
* 尝试使用外部程序解压

解压后要找到一个可执行的单文件bin，可以使用glob查找，同样优先用户配置。

### 配置

使用lock文件可能是比较好的，但是目前不熟悉。可以使用手动的方式，
如使用crontab每天检查更新，要下载安装必需手动执行，而不是放在.zshrc中启动检查。

### 安装与更新

由于在配置时通常配置version=latest，没有固定版本。那就需要保存历史安装的版本，这样才能
在更新时判断是否可更新，对于这样简单的程序，sqlite数据库是最好的选择

可能会存在数据库与安装版本不一致的问题，因为在安装时如果下载到了安装文件夹中，但数据库写入失败时
会与安装出现的版本不一致。我们可以在出现这类问题时重试（有下载解压缓存）。不能直接对其bin做version检查，
由于存在不确定如何调用Bin --version，只能重试。在安装时要允许快速重试处理


## 重构

### Source

支持3种不同的安装源

#### snippet

一个snippet就表示一个远程的bin文件，可以在大部分的bin中正常工作。

##### git release

如git release上的bin，可以直接转化为一些url下载即可。

```toml
[bins.clash]
github = 'd/clash'
release = true
pick = ''
```

##### simple urls

简单的snippet

```toml
[bins.ash]
# pick = a
[[snippets]]
url = 'https://a.com/a'
[[snippets]]
url = 'https://a.com/b'
```

##### command urls

通常，下载更新一个bin的url可能不是固定的，需要访问解析才能确定。snippet应该支持外部计算出url来下载如

```toml
[bins.maven]
snippets.command = 'python3 /a/b.py'
pick = ['a']
```

通过执行命令的执行结果(stdout)找出可用的urls，再pick出bin文件对应的url。

### Git

对于git仓库中的bin，如...p.zsh等，可能会关联仓库中的资源文件，直接clone仓库而不是使用snippet

```toml
[bins.goup]
github = 'a/update-golang'
branch = 'master'
[bin.goup]
pick = '*.sh'
```

### local

通常软件可以通过包管理器安装，如ubuntu的apt-get，编程语言的包管理器rust的cargo，golang的go install，Python的pip等。binaries应该能手动管理这些包

由于包管理器本身就有这些功能，这里只是做一层包装，可以自定义安装命令

```toml
[local.apt-get]
user = 'root'
[local.apt-get.hook]
user = 'root'
command = 'apt-get install {{#each names}}{{this}}{{#unless}} {{/unless}}{{/each}}'
on = ['install']

[local.apt-get.hook]
command = 'apt-get purge {{#each names}}{{this}}{{#unless}} {{/unless}}{{/each}}'
on = ['uninstall']
```
