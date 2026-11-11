import { RootProvider } from 'fumadocs-ui/provider';
import { Inter } from 'next/font/google';
import type { ReactNode } from 'react';
import './global.css';

const inter = Inter({ subsets: ['latin'] });

export const metadata = {
  title: {
    template: '%s | gdtidy',
    default: 'gdtidy — GDScript formatter and linter',
  },
  description: 'Format and lint GDScript, following the official Godot style guide',
};

export default function RootLayout({ children }: { children: ReactNode }) {
  return (
    <html lang="en" className={inter.className} suppressHydrationWarning>
      <body className="flex min-h-screen flex-col">
        <RootProvider
          search={{
            options: {
              type: 'static',
              api: `${process.env.NEXT_PUBLIC_BASE_PATH}/api/search.json`,
            },
          }}
        >
          {children}
        </RootProvider>
      </body>
    </html>
  );
}
