import { themes as prismThemes } from 'prism-react-renderer';
import type { Config } from '@docusaurus/types';
import type * as Preset from '@docusaurus/preset-classic';

const config: Config = {
    title: 'OpenTimsTDF',
    tagline: 'Rust and Python reader for timsTOF .d/ (TDF) bundles',
    favicon: 'img/favicon.ico',

    markdown: {
        mermaid: true,
        hooks: {
            onBrokenMarkdownLinks: 'warn',
        },
    },
    plugins: ['docusaurus-plugin-llms-txt'],
    themes: ['@docusaurus/theme-mermaid'],

    url: 'https://sigilweaver.app',
    baseUrl: '/opentimstdf/docs/',

    organizationName: 'Sigilweaver',
    projectName: 'OpenTimsTDF',

    onBrokenLinks: 'throw',

    i18n: {
        defaultLocale: 'en',
        locales: ['en'],
    },

    presets: [
        [
            'classic',
            {
                docs: {
                    routeBasePath: '/',
                    sidebarPath: './sidebars.ts',
                    editUrl: 'https://github.com/Sigilweaver/OpenTimsTDF/tree/main/docs/',
                },
                blog: false,
                sitemap: {
                    changefreq: 'weekly',
                    priority: 0.5,
                    filename: 'sitemap.xml',
                },
                theme: {
                    customCss: './src/css/custom.css',
                },
            } satisfies Preset.Options,
        ],
    ],

    themeConfig: {
        metadata: [
            { name: 'keywords', content: 'OpenTimsTDF, timsTOF, TDF, mass spectrometry, PASEF, diaPASEF, Rust, Python' },
            { name: 'description', content: 'OpenTimsTDF is a Rust and Python reader for timsTOF .d/ (TDF) bundles.' },
        ],
        colorMode: {
            defaultMode: 'dark',
            disableSwitch: false,
            respectPrefersColorScheme: true,
        },
        navbar: {
            title: 'Sigilweaver',
            logo: {
                alt: 'Sigilweaver logo',
                src: 'img/logo.svg',
                href: 'https://sigilweaver.app',
                target: '_self',
            },
            items: [
                {
                    type: 'dropdown',
                    label: 'OpenTimsTDF',
                    position: 'left',
                    items: [
                        { label: 'OpenProteo', href: 'https://sigilweaver.app/openproteo/docs/' },
                        { label: 'OpenTFRaw (Thermo)', href: 'https://sigilweaver.app/opentfraw/docs/' },
                        { label: 'OpenWRaw (Waters)', href: 'https://sigilweaver.app/openwraw/docs/' },
                    ],
                },
                {
                    href: 'https://docs.rs/OpenTimsTDF',
                    label: 'API (docs.rs)',
                    position: 'right',
                },
                {
                    href: 'https://github.com/Sigilweaver/OpenTimsTDF',
                    label: 'GitHub',
                    position: 'right',
                },
            ],
        },
        footer: {
            style: 'dark',
            links: [
                {
                    title: 'Project',
                    items: [
                        { label: 'GitHub', href: 'https://github.com/Sigilweaver/OpenTimsTDF' },
                        { label: 'Issues', href: 'https://github.com/Sigilweaver/OpenTimsTDF/issues' },
                        { label: 'crates.io', href: 'https://crates.io/crates/OpenTimsTDF' },
                        { label: 'docs.rs', href: 'https://docs.rs/OpenTimsTDF' },
                    ],
                },
                {
                    title: 'Legal',
                    items: [
                        { label: 'Terms of Use', href: 'https://sigilweaver.app/terms' },
                        { label: 'Privacy Policy', href: 'https://sigilweaver.app/privacy' },
                    ],
                },
            ],
            copyright: `Copyright ${new Date().getFullYear()} Sigilweaver Holdings LLC. OpenTimsTDF is Apache-2.0 licensed. Documentation licensed under <a href="https://creativecommons.org/licenses/by-sa/4.0/" target="_blank" rel="noopener noreferrer">CC-BY-SA 4.0</a>.`,
        },
        prism: {
            theme: prismThemes.github,
            darkTheme: prismThemes.dracula,
            additionalLanguages: ['rust', 'toml', 'bash', 'sql'],
        },
    } satisfies Preset.ThemeConfig,
};

export default config;
