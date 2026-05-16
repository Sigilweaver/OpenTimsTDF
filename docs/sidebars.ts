import type { SidebarsConfig } from '@docusaurus/plugin-content-docs';

const sidebars: SidebarsConfig = {
  docsSidebar: [
    'intro',
    'install',
    'quickstart',
    {
      type: 'category',
      label: 'Guide',
      collapsed: false,
      items: [
        'guide/reader',
        'guide/calibration',
        'guide/acquisition-modes',
        'guide/peaks-and-codecs',
      ],
    },
    {
      type: 'category',
      label: 'Format Specification',
      link: { type: 'doc', id: 'format/overview' },
      items: [
        'format/overview',
        'format/tdf-sqlite-schema',
        'format/tdf-bin-block-stream',
        'format/frame-payload-encoding',
        'format/calibration',
        'format/instrument-tables',
        'format/references-and-gaps',
      ],
    },
    {
      type: 'category',
      label: 'Reference',
      items: [
        'reference/api',
        'reference/examples',
      ],
    },
    'changelog',
    'license',
  ],
};

export default sidebars;
