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
* 自动管理：检测配置文件的更改自动安装，下载，更新，卸载，同时要有效率

### 自动管理

在配置好后，应该仅用一条命令完成bin的更改。

#### 多版本

由于配置与软件的更新，可能会使用之前的版本，如何保存

我们在下载时是不知道版本的，只有在安装后才能通过命令执行得到，但是，不同的软件的
命令参数也不同，为每个bin定义也有点麻烦。通常

#### 如何检测配置文件bin的更新

维护一份lock文件，如果发现存在lock文件且配置文件修改时间比lock文件要新，则说明bins可能需要更改，
然后通过比较lock文件与配置文件转换后的是否一致来判断是否应该更新，
在比较old与new配置的bin后如果实际更新了则重安装

在检测到一个bin的source,hook,exes等部分改变后，应该仅更新对应的部分即可（发现source改变后可能需要重新安装），为此，在lock文件中应该保存配置文件的原始信息，并在此基础上添加其它信息

注意：我们不需要部分更新，最消耗时间的是下载阶段，其它如exe,completion,hook时间可以完全重装。

那么如何跳过下载阶段？

snippet.command何时执行？

应该在raw->config时执行，当然，比较lock必要时才执行。
这是由于对于git release而言，转换成的snippet将丢失git source信息，无法在

#### 如何转换配置文件到lock文件

由于配置文件为了方便的格式问题，其实现可能多了许多冗余，直接转换是不可取的。可以使用一个中间配置格式过渡


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

* git release：如git release上的bin，可以直接转化为一些url下载即可。

  ```toml
  [[bins]]
  github = 'd/clash'
  release = true
  ```

* simple urls：简单的snippet

  ```toml
  [[bins]]
  urls = ['https://a.com/a']
  ```

* command urls：通常，下载更新一个bin的url可能不是固定的，需要访问解析才能确定。snippet应该支持外部计算出url来下载。通过执行命令的执行结果(stdout)找出可用的urls，再pick出bin文件对应的url。

  ```toml
  [[bins]]
  command = 'python3 /a/b.py'
  ```

  注意：使用command需要确定何时执行的问题，如果在每次检查或载入配置时执行，可能有点繁琐。如果使用config.lock比较当前配置是否被修改，只有改变配置后执行或install等动作时才执行可避免无意义重复的执行

##### pick

由于在github release和snippet.command中可能需要过滤下载的url，使用正则选择出合适的url

#### Git

对于git仓库中的bin，如...p.zsh等，可能会关联仓库中的资源文件，直接clone仓库而不是使用snippet

```toml
[bins.goup]
github = 'a/update-golang'
branch = 'master'
[bin.goup]
pick = '*.sh'
```

#### local

通常软件可以通过包管理器安装，如ubuntu的apt-get，编程语言的包管理器rust的cargo，golang的go install，Python的pip等。binaries应该能手动管理这些包

由于包管理器本身就有这些功能，这里只是做一层包装，可以自定义安装命令

```toml
[locals.apt-get]
[[locals.apt-get.hooks]]
user = 'root'
command = 'apt-get install {{#each names}}{{this}}{{#unless}} {{/unless}}{{/each}}'
on = ['install']

[[locals.apt-get.hooks]]
command = 'apt-get purge {{#each names}}{{this}}{{#unless}} {{/unless}}{{/each}}'
on = ['uninstall']
```

## .lock文件

* 如何检查lock文件是否应该更新：比较文件修改时间：`.lock < .toml`
  在配置文件.toml后，生成的lock文件时间应该总是滞后于.toml的，如果.toml修改了则时间是`.lock < .toml`，需要更新.lock文件

### 如何处理.toml与.lock的转换

.toml用于定义用户的配置，

.lock用于保存执行后的用户定义配置，方便处理下次改变的部分。如果发现.toml中添加了新的bins则再下次启动进程时自动安装，移除了对应bin时卸载，当更新了一个bins的配置时则更新需要的部分。
而更新update与check需要手动执行。

当然，这种执行可以通过配置改变，也可以手动install，uninstall

如果不这样，直接使用.lock文件保存.toml大部分配置是繁琐的，直接保存一份.toml.copy不是更好。所以在.lock文件中应该放入安装下载后的配置

## 基本支持

### check for updates

对于不同的source有不同的更新方式

#### git

可以通过git fetch等ssh,https,

#### snippet

通用的方法可以发送http head方式检查date header是否变化。但是一个问题url可能是固定的，所有的url不会再更新，所以有command计算url，比较前后的url是否变化来确定更新。

对于一个config.lock来说，一个snippet下载访问过的url可以记录server.date等时间，在更新的时候

* 如果url是可以动态更新的，即可通过比较http head方法的header信息last-modified来更新，这个可以作为默认的更新方式
* 如果url是固定不可更新的，可以通过使用update hooks来手动检查，注入对应的上次snippet下载信息，在command中自己判断，返回运行结果即可判断

通常来说，一个文件的url是固定不能被更新的，不然不同时间访问得到的文件都不一样了，很难处理。有时候可能会使用父路径更新，如`https://archive.apache.org/dist/maven/maven-3/3.8.4/binaries/apache-maven-3.8.4-bin.tar.gz`可以使用`https://archive.apache.org/dist/maven/maven-3/`的head时间判断，但是无法通用，所以使用可注入信息执行手动command的才能解决

另外，对于git release/tags转换的snippet时，应该是内置对应的update hook，无需配置。即在实现时config.lock额外保存git信息，可在更新时判断使用

在考虑如何对git release的snippet实现更新时，发现可以简单的提供一个配置`bins.maven.snippet.check = '$url'`即可满足对应功能

* 在使用snippet时内置检查对last-modified即可
* 在使用git release时可自动设置config.lock文件snippet.check url实现检查更新

当然，也可以支持配置command计算出的url

对于命令式的snippet如

```toml
[bins.maven]
# get url: https://archive.apache.org/dist/maven/maven-3/3.8.4/binaries/apache-maven-3.8.4-bin.tar.gz
snippet.command = 'python3 mvnup.py'
snippet.check = 'https://archive.apache.org/dist/maven/maven-3/'
```

#### local

有些pgk不支持检查更新，可以忽略对应的检查并给出提示
