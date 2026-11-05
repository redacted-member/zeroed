import { useId, type JSX } from 'react';

import type { Picture } from './kit';

import './HelperPicture.scss';

export type HelperSize = 'icon' | 'small' | 'medium' | 'large';

interface HelperPictureProps {
  picture: Picture;
  size?: HelperSize;
}

/**
 * A small explanatory drawing beside a form's option group: what the fields
 * mean, often switching with the option picked. Drawn in the theme colors.
 */
export function HelperPicture({ picture, size = 'medium' }: HelperPictureProps): JSX.Element {
  const titleId = useId();
  return (
    <figure className={`helper-picture helper-picture--${size}`} data-helper={picture.id}>
      <svg
        aria-labelledby={titleId}
        className="helper-picture__svg"
        role="img"
        viewBox={picture.viewBox ?? '0 0 64 48'}
      >
        <title id={titleId}>{picture.alt}</title>
        {picture.draw()}
      </svg>
    </figure>
  );
}
