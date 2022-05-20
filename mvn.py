from urllib.request import urlopen
import re

# https://archive.apache.org/dist/maven/maven-3/
# <!DOCTYPE HTML PUBLIC "-//W3C//DTD HTML 3.2 Final//EN">
# <html>
#  <head>
#   <title>Index of /dist/maven/maven-3</title>
#  </head>
#  <body>
# <h1>Index of /dist/maven/maven-3</h1>
# <pre><img src="/icons/blank.gif" alt="Icon "> <a href="?C=N;O=D">Name</a>                    <a href="?C=M;O=A">Last modified</a>      <a href="?C=S;O=A">Size</a>  <a href="?C=D;O=A">Description</a><hr><img src="/icons/back.gif" alt="[PARENTDIR]"> <a href="/dist/maven/">Parent Directory</a>                             -
# <img src="/icons/folder.gif" alt="[DIR]"> <a href="3.0.4/">3.0.4/</a>                  2012-09-11 09:37    -
# <img src="/icons/folder.gif" alt="[DIR]"> <a href="3.0.5/">3.0.5/</a>                  2020-07-03 04:01    -
# <img src="/icons/folder.gif" alt="[DIR]"> <a href="3.1.0-alpha-1/">3.1.0-alpha-1/</a>          2013-06-07 06:32    -
# <img src="/icons/folder.gif" alt="[DIR]"> <a href="3.1.0/">3.1.0/</a>                  2013-07-14 13:03    -
# <img src="/icons/folder.gif" alt="[DIR]"> <a href="3.1.1/">3.1.1/</a>                  2020-07-03 04:01    -
# <img src="/icons/folder.gif" alt="[DIR]"> <a href="3.2.1/">3.2.1/</a>                  2014-03-10 11:08    -
# <img src="/icons/folder.gif" alt="[DIR]"> <a href="3.2.2/">3.2.2/</a>                  2014-06-26 00:11    -
# <img src="/icons/folder.gif" alt="[DIR]"> <a href="3.2.3/">3.2.3/</a>                  2014-08-15 17:30    -
# <img src="/icons/folder.gif" alt="[DIR]"> <a href="3.2.5/">3.2.5/</a>                  2020-07-03 04:01    -
# <img src="/icons/folder.gif" alt="[DIR]"> <a href="3.3.1/">3.3.1/</a>                  2015-03-17 17:28    -
# <img src="/icons/folder.gif" alt="[DIR]"> <a href="3.3.3/">3.3.3/</a>                  2015-04-28 15:12    -
# <img src="/icons/folder.gif" alt="[DIR]"> <a href="3.3.9/">3.3.9/</a>                  2020-07-03 04:01    -
# <img src="/icons/folder.gif" alt="[DIR]"> <a href="3.5.0-alpha-1/">3.5.0-alpha-1/</a>          2017-02-28 22:25    -
# <img src="/icons/folder.gif" alt="[DIR]"> <a href="3.5.0-beta-1/">3.5.0-beta-1/</a>           2017-03-24 10:48    -
# <img src="/icons/folder.gif" alt="[DIR]"> <a href="3.5.0/">3.5.0/</a>                  2017-10-04 10:47    -
# <img src="/icons/folder.gif" alt="[DIR]"> <a href="3.5.2/">3.5.2/</a>                  2018-05-04 11:19    -
# <img src="/icons/folder.gif" alt="[DIR]"> <a href="3.5.3/">3.5.3/</a>                  2018-05-04 11:19    -
# <img src="/icons/folder.gif" alt="[DIR]"> <a href="3.5.4/">3.5.4/</a>                  2020-07-03 04:01    -
# <img src="/icons/folder.gif" alt="[DIR]"> <a href="3.6.0/">3.6.0/</a>                  2018-10-31 16:43    -
# <img src="/icons/folder.gif" alt="[DIR]"> <a href="3.6.1/">3.6.1/</a>                  2019-09-03 16:54    -
# <img src="/icons/folder.gif" alt="[DIR]"> <a href="3.6.2/">3.6.2/</a>                  2019-09-03 20:13    -
# <img src="/icons/folder.gif" alt="[DIR]"> <a href="3.6.3/">3.6.3/</a>                  2020-07-03 04:01    -
# <img src="/icons/folder.gif" alt="[DIR]"> <a href="3.8.1/">3.8.1/</a>                  2021-04-04 12:24    -
# <img src="/icons/folder.gif" alt="[DIR]"> <a href="3.8.2/">3.8.2/</a>                  2021-08-13 19:53    -
# <img src="/icons/folder.gif" alt="[DIR]"> <a href="3.8.3/">3.8.3/</a>                  2021-10-03 16:34    -
# <img src="/icons/folder.gif" alt="[DIR]"> <a href="3.8.4/">3.8.4/</a>                  2021-11-20 14:43    -
# <img src="/icons/folder.gif" alt="[DIR]"> <a href="3.8.5/">3.8.5/</a>                  2022-03-13 11:21    -
# <hr></pre>
# </body></html>


def bin_list(url='https://archive.apache.org/dist/maven/maven-3/'):
    url = url.strip('/')
    with urlopen(url) as resp:
        body = resp.read().decode('utf-8')
    vers = [m.group(1) for m in re.finditer(r'>(\d+((\.|-)(\d|\w)+)+)', body)]
    vers.sort(reverse=True)

    latest_ver = vers[0]
    list_url = f'{url}/{latest_ver}/binaries'
    return [f'{list_url}/apache-maven-{latest_ver}-bin.tar.gz', f'{list_url}/apache-maven-{latest_ver}-bin.zip']


if __name__ == '__main__':
    url = bin_list()
    print(f'{url}')
