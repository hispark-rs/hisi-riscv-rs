// Populate the sidebar
//
// This is a script, and not included directly in the page, to control the total size of the book.
// The TOC contains an entry for each page, so if each page includes a copy of the TOC,
// the total size of the page becomes O(n**2).
class MDBookSidebarScrollbox extends HTMLElement {
    constructor() {
        super();
    }
    connectedCallback() {
        this.innerHTML = '<ol class="chapter"><li class="chapter-item "><span class="chapter-link-wrapper"><a href="00-introduction.html">引言</a></span></li><li class="chapter-item "><li class="spacer"></li></li><li class="chapter-item "><span class="chapter-link-wrapper"><a href="tutorials/00-index.html"><strong aria-hidden="true">1.</strong> 教程 · Tutorials</a><a class="chapter-fold-toggle"><div>❱</div></a></span><ol class="section"><li class="chapter-item "><span class="chapter-link-wrapper"><a href="tutorials/app/00-index.html"><strong aria-hidden="true">1.1.</strong> 应用开发者</a><a class="chapter-fold-toggle"><div>❱</div></a></span><ol class="section"><li class="chapter-item "><span class="chapter-link-wrapper"><a href="tutorials/app/01-setup.html"><strong aria-hidden="true">1.1.1.</strong> 搭建环境</a></span></li><li class="chapter-item "><span class="chapter-link-wrapper"><a href="tutorials/app/02-first-project.html"><strong aria-hidden="true">1.1.2.</strong> 创建你的第一个工程</a></span></li><li class="chapter-item "><span class="chapter-link-wrapper"><a href="tutorials/app/03-uart.html"><strong aria-hidden="true">1.1.3.</strong> 改造成 UART 程序</a></span></li></ol><li class="chapter-item "><span class="chapter-link-wrapper"><a href="tutorials/contrib/00-index.html"><strong aria-hidden="true">1.2.</strong> 生态贡献者</a><a class="chapter-fold-toggle"><div>❱</div></a></span><ol class="section"><li class="chapter-item "><span class="chapter-link-wrapper"><a href="tutorials/contrib/01-setup.html"><strong aria-hidden="true">1.2.1.</strong> 搭建环境</a></span></li><li class="chapter-item "><span class="chapter-link-wrapper"><a href="tutorials/contrib/02-examples.html"><strong aria-hidden="true">1.2.2.</strong> 构建与运行示例集</a></span></li><li class="chapter-item "><span class="chapter-link-wrapper"><a href="tutorials/contrib/03-hil.html"><strong aria-hidden="true">1.2.3.</strong> 第一次硬件在环测试</a></span></li></ol></li></ol><li class="chapter-item "><span class="chapter-link-wrapper"><a href="how-to/00-index.html"><strong aria-hidden="true">2.</strong> 操作指南 · How-to</a><a class="chapter-fold-toggle"><div>❱</div></a></span><ol class="section"><li class="chapter-item "><span class="chapter-link-wrapper"><a href="how-to/01-install-toolchain.html"><strong aria-hidden="true">2.1.</strong> 安装官方 Rust 工具链</a></span></li><li class="chapter-item "><span class="chapter-link-wrapper"><a href="how-to/02-build-example.html"><strong aria-hidden="true">2.2.</strong> 构建一个示例</a></span></li><li class="chapter-item "><span class="chapter-link-wrapper"><a href="how-to/03-package-image.html"><strong aria-hidden="true">2.3.</strong> 打包成可启动镜像（hisi-fwpkg）</a></span></li><li class="chapter-item "><span class="chapter-link-wrapper"><a href="how-to/04-flash-probe-rs.html"><strong aria-hidden="true">2.4.</strong> 用 probe-rs 烧录到真机</a></span></li><li class="chapter-item "><span class="chapter-link-wrapper"><a href="how-to/05-flash-hisiflash.html"><strong aria-hidden="true">2.5.</strong> 用 hisiflash 烧录到真机</a></span></li><li class="chapter-item "><span class="chapter-link-wrapper"><a href="how-to/06-hardware-runner.html"><strong aria-hidden="true">2.6.</strong> 用硬件 runner 让 cargo run 烧真机</a></span></li><li class="chapter-item "><span class="chapter-link-wrapper"><a href="how-to/07-run-hil-tests.html"><strong aria-hidden="true">2.7.</strong> 运行 HIL 测试</a></span></li><li class="chapter-item "><span class="chapter-link-wrapper"><a href="how-to/08-debug-probe-rs.html"><strong aria-hidden="true">2.8.</strong> 用 probe-rs 调试与读内存</a></span></li><li class="chapter-item "><span class="chapter-link-wrapper"><a href="how-to/09-new-project.html"><strong aria-hidden="true">2.9.</strong> 从模板新建一个工程</a></span></li><li class="chapter-item "><span class="chapter-link-wrapper"><a href="how-to/10-add-driver.html"><strong aria-hidden="true">2.10.</strong> 新增一个外设驱动</a></span></li><li class="chapter-item "><span class="chapter-link-wrapper"><a href="how-to/11-release.html"><strong aria-hidden="true">2.11.</strong> 发布 crate 与父仓 release</a></span></li></ol><li class="chapter-item "><span class="chapter-link-wrapper"><a href="reference/00-index.html"><strong aria-hidden="true">3.</strong> 参考 · Reference</a><a class="chapter-fold-toggle"><div>❱</div></a></span><ol class="section"><li class="chapter-item "><span class="chapter-link-wrapper"><a href="reference/01-memory-map.html"><strong aria-hidden="true">3.1.</strong> 内存映射</a></span></li><li class="chapter-item "><span class="chapter-link-wrapper"><a href="reference/02-examples.html"><strong aria-hidden="true">3.2.</strong> 示例目录与验证标记串</a></span></li><li class="chapter-item "><span class="chapter-link-wrapper"><a href="reference/03-hal-api.html"><strong aria-hidden="true">3.3.</strong> HAL API 总览</a></span></li><li class="chapter-item "><span class="chapter-link-wrapper"><a href="reference/04-peripherals.html"><strong aria-hidden="true">3.4.</strong> 外设清单与覆盖情况</a></span></li><li class="chapter-item "><span class="chapter-link-wrapper"><a href="reference/10-stable-api.html"><strong aria-hidden="true">3.5.</strong> Stable API 清单与门控状态</a></span></li><li class="chapter-item "><span class="chapter-link-wrapper"><a href="reference/05-toolchain.html"><strong aria-hidden="true">3.6.</strong> 工具链与编译目标</a></span></li><li class="chapter-item "><span class="chapter-link-wrapper"><a href="reference/06-image-format.html"><strong aria-hidden="true">3.7.</strong> 应用镜像格式与签名</a></span></li><li class="chapter-item "><span class="chapter-link-wrapper"><a href="reference/07-hil-markers.html"><strong aria-hidden="true">3.8.</strong> HIL 脚本与 runner 环境变量</a></span></li><li class="chapter-item "><span class="chapter-link-wrapper"><a href="reference/08-cli-tools.html"><strong aria-hidden="true">3.9.</strong> CLI 工具速查（hisi-fwpkg / probe-rs）</a></span></li><li class="chapter-item "><span class="chapter-link-wrapper"><a href="reference/09-known-issues.html"><strong aria-hidden="true">3.10.</strong> 已知问题索引</a></span></li><li class="chapter-item "><span class="chapter-link-wrapper"><a href="reference/11-rf-diagnostics.html"><strong aria-hidden="true">3.11.</strong> RF 错误与诊断契约</a></span></li></ol><li class="chapter-item "><span class="chapter-link-wrapper"><a href="explanation/00-index.html"><strong aria-hidden="true">4.</strong> 原理与背景 · Explanation</a><a class="chapter-fold-toggle"><div>❱</div></a></span><ol class="section"><li class="chapter-item "><span class="chapter-link-wrapper"><a href="explanation/01-architecture.html"><strong aria-hidden="true">4.1.</strong> 系统架构总览</a></span></li><li class="chapter-item "><span class="chapter-link-wrapper"><a href="explanation/policies/00-index.html"><strong aria-hidden="true">4.2.</strong> HAL 政策约定</a><a class="chapter-fold-toggle"><div>❱</div></a></span><ol class="section"><li class="chapter-item "><span class="chapter-link-wrapper"><a href="explanation/policies/01-typed-config.html"><strong aria-hidden="true">4.2.1.</strong> 01 类型化配置：能编译就能在硅片上跑</a></span></li><li class="chapter-item "><span class="chapter-link-wrapper"><a href="explanation/policies/02-stable-unstable.html"><strong aria-hidden="true">4.2.2.</strong> 02 稳定 / 不稳定 API 门控</a></span></li><li class="chapter-item "><span class="chapter-link-wrapper"><a href="explanation/policies/03-safe-unsafe-policy.html"><strong aria-hidden="true">4.2.3.</strong> 03 Safe / Unsafe 政策</a></span></li></ol><li class="chapter-item "><span class="chapter-link-wrapper"><a href="explanation/02-boot-flow.html"><strong aria-hidden="true">4.3.</strong> 启动流程：mask ROM → flashboot → app</a></span></li><li class="chapter-item "><span class="chapter-link-wrapper"><a href="explanation/03-hardfloat-toolchain.html"><strong aria-hidden="true">4.4.</strong> 硬浮点工具链</a></span></li><li class="chapter-item "><span class="chapter-link-wrapper"><a href="explanation/04-async-embassy.html"><strong aria-hidden="true">4.5.</strong> async 与 embassy</a></span></li><li class="chapter-item "><span class="chapter-link-wrapper"><a href="explanation/05-secure-boot.html"><strong aria-hidden="true">4.6.</strong> 安全启动与签名</a></span></li><li class="chapter-item "><span class="chapter-link-wrapper"><a href="explanation/06-qemu-model.html"><strong aria-hidden="true">4.7.</strong> QEMU 模型</a></span></li><li class="chapter-item "><span class="chapter-link-wrapper"><a href="explanation/07-hil-framework.html"><strong aria-hidden="true">4.8.</strong> HIL 测试框架</a></span></li><li class="chapter-item "><span class="chapter-link-wrapper"><a href="explanation/08-repository-release-model.html"><strong aria-hidden="true">4.9.</strong> 仓库与发布模型</a></span></li><li class="chapter-item "><span class="chapter-link-wrapper"><a href="explanation/components/00-index.html"><strong aria-hidden="true">4.10.</strong> 组件深入文档</a><a class="chapter-fold-toggle"><div>❱</div></a></span><ol class="section"><li class="chapter-item "><span class="chapter-link-wrapper"><a href="explanation/components/01-overview.html"><strong aria-hidden="true">4.10.1.</strong> 架构总览</a></span></li><li class="chapter-item "><span class="chapter-link-wrapper"><a href="explanation/components/02-ws63-svd.html"><strong aria-hidden="true">4.10.2.</strong> ws63-svd</a></span></li><li class="chapter-item "><span class="chapter-link-wrapper"><a href="explanation/components/03-ws63-pac.html"><strong aria-hidden="true">4.10.3.</strong> ws63-pac</a></span></li><li class="chapter-item "><span class="chapter-link-wrapper"><a href="explanation/components/04-hisi-hal.html"><strong aria-hidden="true">4.10.4.</strong> hisi-hal</a></span></li><li class="chapter-item "><span class="chapter-link-wrapper"><a href="explanation/components/05-hisi-riscv-rt.html"><strong aria-hidden="true">4.10.5.</strong> hisi-riscv-rt</a></span></li><li class="chapter-item "><span class="chapter-link-wrapper"><a href="explanation/components/06-async-embassy.html"><strong aria-hidden="true">4.10.6.</strong> async 与 embassy（深入）</a></span></li><li class="chapter-item "><span class="chapter-link-wrapper"><a href="explanation/components/07-ws63-examples.html"><strong aria-hidden="true">4.10.7.</strong> ws63-examples</a></span></li><li class="chapter-item "><span class="chapter-link-wrapper"><a href="explanation/components/08-ws63-flashboot.html"><strong aria-hidden="true">4.10.8.</strong> ws63-flashboot</a></span></li><li class="chapter-item "><span class="chapter-link-wrapper"><a href="explanation/components/09-ws63-rf.html"><strong aria-hidden="true">4.10.9.</strong> ws63-RF</a></span></li><li class="chapter-item "><span class="chapter-link-wrapper"><a href="explanation/components/10-ws63-guide.html"><strong aria-hidden="true">4.10.10.</strong> ws63-guide</a></span></li><li class="chapter-item "><span class="chapter-link-wrapper"><a href="explanation/components/hi3322-runtime-porting.html"><strong aria-hidden="true">4.10.11.</strong> Hi3322 runtime 移植预研</a></span></li></ol></li></ol></li></ol>';
        // Set the current, active page, and reveal it if it's hidden
        let current_page = document.location.href.toString().split('#')[0].split('?')[0];
        if (current_page.endsWith('/')) {
            current_page += 'index.html';
        }
        const links = Array.prototype.slice.call(this.querySelectorAll('a'));
        const l = links.length;
        for (let i = 0; i < l; ++i) {
            const link = links[i];
            const href = link.getAttribute('href');
            if (href && !href.startsWith('#') && !/^(?:[a-z+]+:)?\/\//.test(href)) {
                link.href = path_to_root + href;
            }
            // The 'index' page is supposed to alias the first chapter in the book.
            // Check both with and without the '.html' suffix to be robust against pretty URLs
            if (link.href.replace(/\.html$/, '') === current_page.replace(/\.html$/, '')
                || i === 0
                && path_to_root === ''
                && current_page.endsWith('/index.html')) {
                link.classList.add('active');
                let parent = link.parentElement;
                while (parent) {
                    if (parent.tagName === 'LI' && parent.classList.contains('chapter-item')) {
                        parent.classList.add('expanded');
                    }
                    parent = parent.parentElement;
                }
            }
        }
        // Track and set sidebar scroll position
        this.addEventListener('click', e => {
            if (e.target.tagName === 'A') {
                const clientRect = e.target.getBoundingClientRect();
                const sidebarRect = this.getBoundingClientRect();
                sessionStorage.setItem('sidebar-scroll-offset', clientRect.top - sidebarRect.top);
            }
        }, { passive: true });
        const sidebarScrollOffset = sessionStorage.getItem('sidebar-scroll-offset');
        sessionStorage.removeItem('sidebar-scroll-offset');
        if (sidebarScrollOffset !== null) {
            // preserve sidebar scroll position when navigating via links within sidebar
            const activeSection = this.querySelector('.active');
            if (activeSection) {
                const clientRect = activeSection.getBoundingClientRect();
                const sidebarRect = this.getBoundingClientRect();
                const currentOffset = clientRect.top - sidebarRect.top;
                this.scrollTop += currentOffset - parseFloat(sidebarScrollOffset);
            }
        } else {
            // scroll sidebar to current active section when navigating via
            // 'next/previous chapter' buttons
            const activeSection = document.querySelector('#mdbook-sidebar .active');
            if (activeSection) {
                activeSection.scrollIntoView({ block: 'center' });
            }
        }
        // Toggle buttons
        const sidebarAnchorToggles = document.querySelectorAll('.chapter-fold-toggle');
        function toggleSection(ev) {
            ev.currentTarget.parentElement.parentElement.classList.toggle('expanded');
        }
        Array.from(sidebarAnchorToggles).forEach(el => {
            el.addEventListener('click', toggleSection);
        });
    }
}
window.customElements.define('mdbook-sidebar-scrollbox', MDBookSidebarScrollbox);


// ---------------------------------------------------------------------------
// Support for dynamically adding headers to the sidebar.

(function() {
    // This is used to detect which direction the page has scrolled since the
    // last scroll event.
    let lastKnownScrollPosition = 0;
    // This is the threshold in px from the top of the screen where it will
    // consider a header the "current" header when scrolling down.
    const defaultDownThreshold = 150;
    // Same as defaultDownThreshold, except when scrolling up.
    const defaultUpThreshold = 300;
    // The threshold is a virtual horizontal line on the screen where it
    // considers the "current" header to be above the line. The threshold is
    // modified dynamically to handle headers that are near the bottom of the
    // screen, and to slightly offset the behavior when scrolling up vs down.
    let threshold = defaultDownThreshold;
    // This is used to disable updates while scrolling. This is needed when
    // clicking the header in the sidebar, which triggers a scroll event. It
    // is somewhat finicky to detect when the scroll has finished, so this
    // uses a relatively dumb system of disabling scroll updates for a short
    // time after the click.
    let disableScroll = false;
    // Array of header elements on the page.
    let headers;
    // Array of li elements that are initially collapsed headers in the sidebar.
    // I'm not sure why eslint seems to have a false positive here.
    // eslint-disable-next-line prefer-const
    let headerToggles = [];
    // This is a debugging tool for the threshold which you can enable in the console.
    let thresholdDebug = false;

    // Updates the threshold based on the scroll position.
    function updateThreshold() {
        const scrollTop = window.pageYOffset || document.documentElement.scrollTop;
        const windowHeight = window.innerHeight;
        const documentHeight = document.documentElement.scrollHeight;

        // The number of pixels below the viewport, at most documentHeight.
        // This is used to push the threshold down to the bottom of the page
        // as the user scrolls towards the bottom.
        const pixelsBelow = Math.max(0, documentHeight - (scrollTop + windowHeight));
        // The number of pixels above the viewport, at least defaultDownThreshold.
        // Similar to pixelsBelow, this is used to push the threshold back towards
        // the top when reaching the top of the page.
        const pixelsAbove = Math.max(0, defaultDownThreshold - scrollTop);
        // How much the threshold should be offset once it gets close to the
        // bottom of the page.
        const bottomAdd = Math.max(0, windowHeight - pixelsBelow - defaultDownThreshold);
        let adjustedBottomAdd = bottomAdd;

        // Adjusts bottomAdd for a small document. The calculation above
        // assumes the document is at least twice the windowheight in size. If
        // it is less than that, then bottomAdd needs to be shrunk
        // proportional to the difference in size.
        if (documentHeight < windowHeight * 2) {
            const maxPixelsBelow = documentHeight - windowHeight;
            const t = 1 - pixelsBelow / Math.max(1, maxPixelsBelow);
            const clamp = Math.max(0, Math.min(1, t));
            adjustedBottomAdd *= clamp;
        }

        let scrollingDown = true;
        if (scrollTop < lastKnownScrollPosition) {
            scrollingDown = false;
        }

        if (scrollingDown) {
            // When scrolling down, move the threshold up towards the default
            // downwards threshold position. If near the bottom of the page,
            // adjustedBottomAdd will offset the threshold towards the bottom
            // of the page.
            const amountScrolledDown = scrollTop - lastKnownScrollPosition;
            const adjustedDefault = defaultDownThreshold + adjustedBottomAdd;
            threshold = Math.max(adjustedDefault, threshold - amountScrolledDown);
        } else {
            // When scrolling up, move the threshold down towards the default
            // upwards threshold position. If near the bottom of the page,
            // quickly transition the threshold back up where it normally
            // belongs.
            const amountScrolledUp = lastKnownScrollPosition - scrollTop;
            const adjustedDefault = defaultUpThreshold - pixelsAbove
                + Math.max(0, adjustedBottomAdd - defaultDownThreshold);
            threshold = Math.min(adjustedDefault, threshold + amountScrolledUp);
        }

        if (documentHeight <= windowHeight) {
            threshold = 0;
        }

        if (thresholdDebug) {
            const id = 'mdbook-threshold-debug-data';
            let data = document.getElementById(id);
            if (data === null) {
                data = document.createElement('div');
                data.id = id;
                data.style.cssText = `
                    position: fixed;
                    top: 50px;
                    right: 10px;
                    background-color: 0xeeeeee;
                    z-index: 9999;
                    pointer-events: none;
                `;
                document.body.appendChild(data);
            }
            data.innerHTML = `
                <table>
                  <tr><td>documentHeight</td><td>${documentHeight.toFixed(1)}</td></tr>
                  <tr><td>windowHeight</td><td>${windowHeight.toFixed(1)}</td></tr>
                  <tr><td>scrollTop</td><td>${scrollTop.toFixed(1)}</td></tr>
                  <tr><td>pixelsAbove</td><td>${pixelsAbove.toFixed(1)}</td></tr>
                  <tr><td>pixelsBelow</td><td>${pixelsBelow.toFixed(1)}</td></tr>
                  <tr><td>bottomAdd</td><td>${bottomAdd.toFixed(1)}</td></tr>
                  <tr><td>adjustedBottomAdd</td><td>${adjustedBottomAdd.toFixed(1)}</td></tr>
                  <tr><td>scrollingDown</td><td>${scrollingDown}</td></tr>
                  <tr><td>threshold</td><td>${threshold.toFixed(1)}</td></tr>
                </table>
            `;
            drawDebugLine();
        }

        lastKnownScrollPosition = scrollTop;
    }

    function drawDebugLine() {
        if (!document.body) {
            return;
        }
        const id = 'mdbook-threshold-debug-line';
        const existingLine = document.getElementById(id);
        if (existingLine) {
            existingLine.remove();
        }
        const line = document.createElement('div');
        line.id = id;
        line.style.cssText = `
            position: fixed;
            top: ${threshold}px;
            left: 0;
            width: 100vw;
            height: 2px;
            background-color: red;
            z-index: 9999;
            pointer-events: none;
        `;
        document.body.appendChild(line);
    }

    function mdbookEnableThresholdDebug() {
        thresholdDebug = true;
        updateThreshold();
        drawDebugLine();
    }

    window.mdbookEnableThresholdDebug = mdbookEnableThresholdDebug;

    // Updates which headers in the sidebar should be expanded. If the current
    // header is inside a collapsed group, then it, and all its parents should
    // be expanded.
    function updateHeaderExpanded(currentA) {
        // Add expanded to all header-item li ancestors.
        let current = currentA.parentElement;
        while (current) {
            if (current.tagName === 'LI' && current.classList.contains('header-item')) {
                current.classList.add('expanded');
            }
            current = current.parentElement;
        }
    }

    // Updates which header is marked as the "current" header in the sidebar.
    // This is done with a virtual Y threshold, where headers at or below
    // that line will be considered the current one.
    function updateCurrentHeader() {
        if (!headers || !headers.length) {
            return;
        }

        // Reset the classes, which will be rebuilt below.
        const els = document.getElementsByClassName('current-header');
        for (const el of els) {
            el.classList.remove('current-header');
        }
        for (const toggle of headerToggles) {
            toggle.classList.remove('expanded');
        }

        // Find the last header that is above the threshold.
        let lastHeader = null;
        for (const header of headers) {
            const rect = header.getBoundingClientRect();
            if (rect.top <= threshold) {
                lastHeader = header;
            } else {
                break;
            }
        }
        if (lastHeader === null) {
            lastHeader = headers[0];
            const rect = lastHeader.getBoundingClientRect();
            const windowHeight = window.innerHeight;
            if (rect.top >= windowHeight) {
                return;
            }
        }

        // Get the anchor in the summary.
        const href = '#' + lastHeader.id;
        const a = [...document.querySelectorAll('.header-in-summary')]
            .find(element => element.getAttribute('href') === href);
        if (!a) {
            return;
        }

        a.classList.add('current-header');

        updateHeaderExpanded(a);
    }

    // Updates which header is "current" based on the threshold line.
    function reloadCurrentHeader() {
        if (disableScroll) {
            return;
        }
        updateThreshold();
        updateCurrentHeader();
    }


    // When clicking on a header in the sidebar, this adjusts the threshold so
    // that it is located next to the header. This is so that header becomes
    // "current".
    function headerThresholdClick(event) {
        // See disableScroll description why this is done.
        disableScroll = true;
        setTimeout(() => {
            disableScroll = false;
        }, 100);
        // requestAnimationFrame is used to delay the update of the "current"
        // header until after the scroll is done, and the header is in the new
        // position.
        requestAnimationFrame(() => {
            requestAnimationFrame(() => {
                // Closest is needed because if it has child elements like <code>.
                const a = event.target.closest('a');
                const href = a.getAttribute('href');
                const targetId = href.substring(1);
                const targetElement = document.getElementById(targetId);
                if (targetElement) {
                    threshold = targetElement.getBoundingClientRect().bottom;
                    updateCurrentHeader();
                }
            });
        });
    }

    // Takes the nodes from the given head and copies them over to the
    // destination, along with some filtering.
    function filterHeader(source, dest) {
        const clone = source.cloneNode(true);
        clone.querySelectorAll('mark').forEach(mark => {
            mark.replaceWith(...mark.childNodes);
        });
        dest.append(...clone.childNodes);
    }

    // Scans page for headers and adds them to the sidebar.
    document.addEventListener('DOMContentLoaded', function() {
        const activeSection = document.querySelector('#mdbook-sidebar .active');
        if (activeSection === null) {
            return;
        }

        const main = document.getElementsByTagName('main')[0];
        headers = Array.from(main.querySelectorAll('h2, h3, h4, h5, h6'))
            .filter(h => h.id !== '' && h.children.length && h.children[0].tagName === 'A');

        if (headers.length === 0) {
            return;
        }

        // Build a tree of headers in the sidebar.

        const stack = [];

        const firstLevel = parseInt(headers[0].tagName.charAt(1));
        for (let i = 1; i < firstLevel; i++) {
            const ol = document.createElement('ol');
            ol.classList.add('section');
            if (stack.length > 0) {
                stack[stack.length - 1].ol.appendChild(ol);
            }
            stack.push({level: i + 1, ol: ol});
        }

        // The level where it will start folding deeply nested headers.
        const foldLevel = 3;

        for (let i = 0; i < headers.length; i++) {
            const header = headers[i];
            const level = parseInt(header.tagName.charAt(1));

            const currentLevel = stack[stack.length - 1].level;
            if (level > currentLevel) {
                // Begin nesting to this level.
                for (let nextLevel = currentLevel + 1; nextLevel <= level; nextLevel++) {
                    const ol = document.createElement('ol');
                    ol.classList.add('section');
                    const last = stack[stack.length - 1];
                    const lastChild = last.ol.lastChild;
                    // Handle the case where jumping more than one nesting
                    // level, which doesn't have a list item to place this new
                    // list inside of.
                    if (lastChild) {
                        lastChild.appendChild(ol);
                    } else {
                        last.ol.appendChild(ol);
                    }
                    stack.push({level: nextLevel, ol: ol});
                }
            } else if (level < currentLevel) {
                while (stack.length > 1 && stack[stack.length - 1].level > level) {
                    stack.pop();
                }
            }

            const li = document.createElement('li');
            li.classList.add('header-item');
            li.classList.add('expanded');
            if (level < foldLevel) {
                li.classList.add('expanded');
            }
            const span = document.createElement('span');
            span.classList.add('chapter-link-wrapper');
            const a = document.createElement('a');
            span.appendChild(a);
            a.href = '#' + header.id;
            a.classList.add('header-in-summary');
            filterHeader(header.children[0], a);
            a.addEventListener('click', headerThresholdClick);
            const nextHeader = headers[i + 1];
            if (nextHeader !== undefined) {
                const nextLevel = parseInt(nextHeader.tagName.charAt(1));
                if (nextLevel > level && level >= foldLevel) {
                    const toggle = document.createElement('a');
                    toggle.classList.add('chapter-fold-toggle');
                    toggle.classList.add('header-toggle');
                    toggle.addEventListener('click', () => {
                        li.classList.toggle('expanded');
                    });
                    const toggleDiv = document.createElement('div');
                    toggleDiv.textContent = '❱';
                    toggle.appendChild(toggleDiv);
                    span.appendChild(toggle);
                    headerToggles.push(li);
                }
            }
            li.appendChild(span);

            const currentParent = stack[stack.length - 1];
            currentParent.ol.appendChild(li);
        }

        const onThisPage = document.createElement('div');
        onThisPage.classList.add('on-this-page');
        onThisPage.append(stack[0].ol);
        const activeItemSpan = activeSection.parentElement;
        activeItemSpan.after(onThisPage);
    });

    document.addEventListener('DOMContentLoaded', reloadCurrentHeader);
    document.addEventListener('scroll', reloadCurrentHeader, { passive: true });
})();

